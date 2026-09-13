//! MCP 端点：Model Context Protocol over Streamable HTTP（POST /mcp，无状态）。
//!
//! 对外只暴露**一个工具** `helm`（渐进式编目见 application::mcp_registry）。
//! tools/call 把 op 翻译成对自身 HTTP API 的 loopback 调用（带调用者的 Bearer）——
//! MCP 面 == HTTP 面，鉴权/审计/错误映射全部复用既有路由，零重复实现。
//!
//! 协议：JSON-RPC 2.0，支持 initialize / notifications/initialized / tools/list /
//! tools/call / ping；请求可为单对象或 batch 数组。响应 application/json
//! （Streamable HTTP 规范允许非 SSE 响应；无会话，客户端无需维持连接）。

use crate::application::mcp_registry::{self, Os};
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::{Value, json};
use std::sync::OnceLock;

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
        "tools/list" => Ok(tools_list(key_scopes)),
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

/// 单工具：描述里内嵌 scope 裁剪后的 op 索引（渐进式第一层）。
fn tools_list(key_scopes: &[String]) -> Value {
    json!({
        "tools": [{
            "name": TOOL_NAME,
            "description": mcp_registry::tool_description(key_scopes),
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

/// tools/call：op 分发（catalog 内联处理，其余翻译为 loopback HTTP）。
async fn tools_call(
    state: &AppState,
    key_scopes: &[String],
    token: &str,
    params: Option<&Value>,
) -> Result<Value, JsonRpcError> {
    let args = params
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.as_object())
        .ok_or_else(|| JsonRpcError::new(-32602, "tools/call requires arguments object"))?;

    let op = args
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::new(-32602, "missing 'op'"))?;

    // 通用 op：编目（scope 裁剪 + domain/os 过滤）
    if op == "catalog" {
        let domain = args.get("domain").and_then(|v| v.as_str());
        let os = match args.get("os").and_then(|v| v.as_str()) {
            Some(s) => Some(
                Os::parse(s)
                    .ok_or_else(|| JsonRpcError::new(-32602, &format!("unknown os: {s}")))?,
            ),
            None => None,
        };
        return tool_ok(mcp_registry::catalog_json(key_scopes, domain, os));
    }

    let Some(def) = mcp_registry::find(op) else {
        let names: Vec<&str> = mcp_registry::allowed_ops(key_scopes, None)
            .iter()
            .map(|o| o.name)
            .take(12)
            .collect();
        return Err(JsonRpcError::new(
            -32602,
            &format!("unknown op: {op}；可用的 op 见 catalog（部分：{names:?}）"),
        ));
    };

    // scope 双保险（编目已裁剪，防目录缓存/竞态）
    if !key_scopes.is_empty() && !key_scopes.iter().any(|s| s == def.scope) {
        return Ok(mcp_error_result(&format!(
            "凭证缺少 '{}' scope，无法执行 {op}；请管理员在控制台重新签发",
            def.scope
        )));
    }

    // os 检查：OS 专属 op 需要匹配
    let caller_os = match args.get("os").and_then(|v| v.as_str()) {
        Some(s) => Os::parse(s),
        None => None,
    };
    if def.os != Os::Any {
        let hint = format!(
            "{op} 仅支持 {}；os 参数请传 \"{}\"",
            def.os.as_str(),
            def.os.as_str()
        );
        match caller_os {
            Some(os) if os == def.os => {}
            _ => return Ok(mcp_error_result(&hint)),
        }
    } else if caller_os == Some(Os::Windows) || caller_os == Some(Os::Linux) {
        // any op 带了 os 参数：不拦截（可能用于语义提示）
    }

    // 组装 loopback 请求：路径占位符从 args 取，剩余 GET→query / 其余→JSON body
    let mut args_map = args.get("args").cloned().unwrap_or_else(|| json!({}));
    let mut path = def.path.to_string();
    for ph in mcp_registry::placeholders(def.path) {
        let val = args_map
            .get(ph)
            .cloned()
            .or_else(|| args.get("target").cloned().filter(|_| ph == "id"))
            .ok_or_else(|| {
                JsonRpcError::new(-32602, &format!("missing args.{ph} for {op}（见 catalog）"))
            })?;
        let rendered = match &val {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        path = path.replace(&format!("{{{ph}}}"), &rendered);
        args_map.as_object_mut().unwrap().remove(ph);
    }
    // 特例：services.action 的 {action} 占位符也支持 target 传入
    let url = format!("http://127.0.0.1:{}{path}", state.http_port);

    let mut req = http_client()
        .request(
            reqwest::Method::from_bytes(def.method.as_bytes()).expect("valid http method"),
            &url,
        )
        .bearer_auth(token)
        .timeout(std::time::Duration::from_secs(RELAY_TIMEOUT_SECS));
    if def.method == "GET" || def.method == "DELETE" {
        // 剩余参数走 query（DELETE 一般无剩余）
        if let Some(obj) = args_map.as_object() {
            for (k, v) in obj {
                req = req.query(&[(k, value_to_query(v))]);
            }
        }
    } else if args_map.as_object().is_some_and(|o| !o.is_empty()) {
        req = req.json(&args_map);
    } else {
        req = req.json(&json!({}));
    }

    let resp = req
        .send()
        .await
        .map_err(|e| JsonRpcError::new(-32000, &format!("loopback call failed: {e}")))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .unwrap_or_else(|_| String::from("<unreadable body>"));

    if !status.is_success() {
        let msg = summarize_error(status.as_u16(), &text);
        return Ok(mcp_error_result(&msg));
    }

    // 成功：原样透传 JSON（非 JSON 响应包一层），尾部附渐进提示
    let mut payload: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "_available_ops".into(),
            json!(
                mcp_registry::allowed_ops(key_scopes, None)
                    .iter()
                    .map(|o| o.name)
                    .collect::<Vec<_>>()
            ),
        );
    }
    tool_ok(payload)
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
