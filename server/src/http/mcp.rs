//! MCP 端点：Model Context Protocol over Streamable HTTP（POST /mcp，无状态）。
//!
//! 对外只暴露**一个工具** `helm`（渐进式编目见 application::mcp_registry）。
//! tools/call 把 op 翻译成对自身 HTTP API 的 loopback 调用（带调用者的 Bearer）——
//! MCP 面 == HTTP 面，鉴权/审计/错误映射全部复用既有路由，零重复实现。
//!
//! 协议：JSON-RPC 2.0，支持 initialize / notifications/initialized / tools/list /
//! tools/call / ping；请求可为单对象或 batch 数组。响应 application/json
//! （Streamable HTTP 规范允许非 SSE 响应；无会话，客户端无需维持连接）。

use crate::application::mcp_registry;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::OnceLock;

mod tool_call;

use tool_call::tools_call;

const PROTOCOL_VERSION: &str = "2025-06-18";
const TOOL_NAME: &str = "helm";
/// loopback 自调用超时（证据包等慢操作上限）。
const RELAY_TIMEOUT_SECS: u64 = 120;

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(RELAY_TIMEOUT_SECS))
            .build()
            .expect("reqwest client")
    })
}

/// POST /mcp：鉴权 → JSON-RPC 分发。顶层路由，鉴权在此自理（同 WS 端点模式）。
pub async fn mcp(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    let token = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .unwrap_or("");
    let claims = match crate::http::auth::verify_bearer_token(&state, token).await {
        Ok(c) => c,
        Err(e) => {
            let (status, msg) = match e {
                crate::domain::Error::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
                other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
            };
            return (status, Json(JsonRpcError::new(-32000, &msg).into_value())).into_response();
        }
    };

    let parsed: Result<Value, _> = serde_json::from_slice(&body);
    let payload = match parsed {
        Ok(v) => v,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(JsonRpcError::new(-32700, &format!("parse error: {e}")).into_value()),
            )
                .into_response();
        }
    };

    match payload {
        Value::Array(batch) => {
            let mut responses = Vec::with_capacity(batch.len());
            for item in batch {
                if let Some(resp) = dispatch(&state, &claims.scopes, token, item).await {
                    responses.push(resp);
                }
            }
            if responses.is_empty() {
                StatusCode::ACCEPTED.into_response()
            } else {
                (StatusCode::OK, Json(responses)).into_response()
            }
        }
        item => match dispatch(&state, &claims.scopes, token, item).await {
            Some(resp) => (StatusCode::OK, Json(resp)).into_response(),
            None => StatusCode::ACCEPTED.into_response(),
        },
    }
}

/// 处理单个 JSON-RPC 消息。通知（无 id）返回 None（HTTP 202）。
async fn dispatch(
    state: &AppState,
    key_scopes: &[String],
    token: &str,
    msg: Value,
) -> Option<Value> {
    let id = msg.get("id").cloned();
    let method = msg.get("method").and_then(|m| m.as_str()).map(String::from);

    let Some(method) = method else {
        // 纯通知或畸形消息：有 id 的畸形按 -32600 回，通知静默
        return if id.is_some() {
            Some(
                JsonRpcError::new(-32600, "missing method")
                    .with_id(id)
                    .into_value(),
            )
        } else {
            None
        };
    };
    let is_notification = id.is_none();

    let result: Result<Value, JsonRpcError> = match method.as_str() {
        "initialize" => Ok(initialize_result()),
        "notifications/initialized" => Ok(json!(null)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(tools_list(state, key_scopes)),
        "tools/call" => tools_call(state, key_scopes, token, msg.get("params")).await,
        "prompts/list" => Ok(json!({ "prompts": [] })),
        "resources/list" => Ok(json!({ "resources": [] })),
        other => Err(JsonRpcError::new(
            -32601,
            &format!("method not found: {other}"),
        )),
    };

    // 通知不需要响应（即使结果出错）
    if is_notification {
        return None;
    }
    let id = id.unwrap_or(Value::Null);
    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(err) => err.with_id(Some(id)).into_value(),
    })
}

fn initialize_result() -> Value {
    json!({
        "protocolVersion": PROTOCOL_VERSION,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": {
            "name": "helm",
            "version": env!("CARGO_PKG_VERSION"),
            "title": "helm 集中式运维平台"
        },
        "instructions": "通过 helm 工具的 op 参数操作平台；先调 {\"op\":\"catalog\"} 看可用操作"
    })
}

/// 单工具：描述里内嵌 scope 裁剪后的 op 索引（渐进式第一层，P002 T4 按 tier 收缩）。
fn tools_list(state: &AppState, key_scopes: &[String]) -> Value {
    json!({
        "tools": [{
            "name": TOOL_NAME,
            "description": mcp_registry::tool_description(key_scopes, state.mcp_tier),
            "inputSchema": {
                "type": "object",
                "properties": {
                    "op": { "type": "string", "description": "操作名，如 exec.run / catalog；完整目录见 catalog" },
                    "os": { "type": "string", "enum": ["linux", "windows"], "description": "目标 OS（OS 专属操作建议传）" },
                    "args": { "type": "object", "description": "操作参数；路径占位符（如 {id}）直接作为键", "additionalProperties": true }
                },
                "required": ["op"]
            }
        }]
    })
}

/// 非 2xx 的工具错误消息：4xx/5xx 区分，403 带签发指引。
fn summarize_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| match v.get("error") {
            // http::error 形状：{error: {code, message}} 或 {error: "..."}
            Some(e) => Some(match e {
                Value::String(s) => s.clone(),
                other => other
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(&other.to_string())
                    .to_string(),
            }),
            // 部分端点直接回 {code, message}
            None => v.get("message").and_then(|m| m.as_str()).map(String::from),
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    if status == 403 {
        format!("403：{detail}（scope 不足时需管理员在控制台签发对应凭证）")
    } else {
        format!("{status}：{detail}")
    }
}

fn mcp_error_result(msg: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": msg }],
        "isError": true
    })
}

fn tool_ok(payload: Value) -> Result<Value, JsonRpcError> {
    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string(&payload).unwrap_or_default() }],
        "isError": false
    }))
}

fn value_to_query(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// JSON-RPC 错误对象。
struct JsonRpcError {
    code: i32,
    message: String,
    id: Option<Value>,
}

impl JsonRpcError {
    fn new(code: i32, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
            id: None,
        }
    }

    fn with_id(mut self, id: Option<Value>) -> Self {
        self.id = id;
        self
    }

    fn into_value(self) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": self.id,
            "error": { "code": self.code, "message": self.message }
        })
    }
}
