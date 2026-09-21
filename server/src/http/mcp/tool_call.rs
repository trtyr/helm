//! tools/call 的**翻译层**：把 `{op, os, args}` 翻译成对自身 HTTP API 的 loopback 调用。
//!
//! 自 `http/mcp.rs` 拆出（G7：该文件因工具调用拆分涨到 440 行越界）。本模块只回答
//! 「一个 op 怎么变成一次 HTTP 调用、结果怎么回给 AI」；JSON-RPC 信封（initialize /
//! tools/list / dispatch / 错误对象）仍在父模块。
//!
//! 入口：[`tools_call`]（父模块 `dispatch` 调用）。私有项（`JsonRpcError`、`tool_ok`、
//! `mcp_error_result`、`http_client`、`summarize_error`、`value_to_query`、
//! `RELAY_TIMEOUT_SECS`）来自父模块——子模块可见祖先模块的私有项。

use super::{
    JsonRpcError, RELAY_TIMEOUT_SECS, http_client, mcp_error_result, summarize_error, tool_ok,
    value_to_query,
};
use crate::application::mcp_registry::{self, Os};
use crate::http::AppState;
use serde_json::{Value, json};

/// tools/call：op 分发（catalog 内联处理，其余翻译为 loopback HTTP）。
///
/// 只做编排：参数解析 → catalog 短路 → op 解析 → 两道校验 → 路径渲染 → loopback 调用
/// → 错误归一 → 附渐进提示。各步骤实现见其后的具名函数。
pub(super) async fn tools_call(
    state: &AppState,
    key_scopes: &[String],
    token: &str,
    params: Option<&Value>,
) -> Result<Value, JsonRpcError> {
    let args = call_args(params)?;
    let op = args
        .get("op")
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError::new(-32602, "missing 'op'"))?;

    // 通用 op：编目（scope 裁剪 + domain/os 过滤）
    if op == "catalog" {
        return tool_ok(mcp_registry::catalog_json(
            key_scopes,
            args.get("domain").and_then(|v| v.as_str()),
            parse_os_arg(args.get("os"))?,
            state.mcp_tier,
        ));
    }

    let def = resolve_op(op, key_scopes)?;
    if let Some(err) = scope_error(def, key_scopes, op) {
        return Ok(err);
    }
    if let Some(err) = os_error(def, parse_os_arg(args.get("os"))?, op) {
        return Ok(err);
    }

    let mut args_map = args.get("args").cloned().unwrap_or_else(|| json!({}));
    let path = render_path(def, args, &mut args_map, op)?;
    let (status, text) = call_loopback(state, def, token, &path, &args_map).await?;
    if !status.is_success() {
        return Ok(mcp_error_result(&summarize_error(status.as_u16(), &text)));
    }
    tool_ok(with_available_ops(text, key_scopes))
}

/// tools/call 的 arguments 对象（缺失即协议错误）。
fn call_args(params: Option<&Value>) -> Result<&serde_json::Map<String, Value>, JsonRpcError> {
    params
        .and_then(|p| p.get("arguments"))
        .and_then(|a| a.as_object())
        .ok_or_else(|| JsonRpcError::new(-32602, "tools/call requires arguments object"))
}

/// 解析 `os` 参数（未知取值即协议错误）。
fn parse_os_arg(v: Option<&Value>) -> Result<Option<Os>, JsonRpcError> {
    match v.and_then(|x| x.as_str()) {
        Some(s) => Os::parse(s)
            .map(Some)
            .ok_or_else(|| JsonRpcError::new(-32602, &format!("unknown os: {s}"))),
        None => Ok(None),
    }
}

/// 按名找 op；未知 op 的错误里带前 12 个可用 op 作提示。
fn resolve_op(
    op: &str,
    key_scopes: &[String],
) -> Result<&'static mcp_registry::OpDef, JsonRpcError> {
    match mcp_registry::find(op) {
        Some(def) => Ok(def),
        None => {
            let names: Vec<&str> = mcp_registry::allowed_ops(key_scopes, None)
                .iter()
                .map(|o| o.name)
                .take(12)
                .collect();
            Err(JsonRpcError::new(
                -32602,
                &format!("unknown op: {op}；可用的 op 见 catalog（部分：{names:?}）"),
            ))
        }
    }
}

/// scope 双保险（编目已裁剪，防目录缓存/竞态）：不满足则返回可直接回给 AI 的错误结果。
fn scope_error(def: &mcp_registry::OpDef, key_scopes: &[String], op: &str) -> Option<Value> {
    if key_scopes.is_empty() || key_scopes.iter().any(|s| s == def.scope) {
        return None;
    }
    Some(mcp_error_result(&format!(
        "凭证缺少 '{}' scope，无法执行 {op}；请管理员在控制台重新签发",
        def.scope
    )))
}

/// os 检查：OS 专属 op 必须匹配；`Any` op 带 os 参数不拦截（可能只作语义提示）。
fn os_error(def: &mcp_registry::OpDef, caller_os: Option<Os>, op: &str) -> Option<Value> {
    if def.os == Os::Any {
        return None;
    }
    match caller_os {
        Some(os) if os == def.os => None,
        _ => Some(mcp_error_result(&format!(
            "{op} 仅支持 {}；os 参数请传 \"{}\"",
            def.os.as_str(),
            def.os.as_str()
        ))),
    }
}

/// 渲染 loopback 路径：路径占位符从 args 取（`{id}` 亦可由 `target` 提供），取走后从
/// `args_map` 删除，剩余的留给 query/body。
fn render_path(
    def: &mcp_registry::OpDef,
    args: &serde_json::Map<String, Value>,
    args_map: &mut Value,
    op: &str,
) -> Result<String, JsonRpcError> {
    let mut path = def.path.to_string();
    for ph in mcp_registry::placeholders(def.path) {
        // 特例：services.action 的 {action} 占位符也支持 target 传入
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
    Ok(path)
}

/// 发起 loopback HTTP 调用：GET/DELETE 把剩余参数放 query，其余放 JSON body。
async fn call_loopback(
    state: &AppState,
    def: &mcp_registry::OpDef,
    token: &str,
    path: &str,
    args_map: &Value,
) -> Result<(reqwest::StatusCode, String), JsonRpcError> {
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
        req = req.json(args_map);
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
    Ok((status, text))
}

/// 成功响应：原样透传 JSON（非 JSON 响应包一层），尾部附渐进提示 `_available_ops`。
fn with_available_ops(text: String, key_scopes: &[String]) -> Value {
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
    payload
}
