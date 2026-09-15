//! scope 编目完整性测试：openapi 契约里的每条 /api/v1 路由，要么在
//! `http::auth::scope_for` 编目（API key 按 scope 放行），要么属于豁免族
//! （api-keys/账号 = JWT-only；login = 公开）。漏编目的新路由会在此失败，
//! 防止 fail-closed 规则悄悄挡掉合法的 API key 调用。

use helm_server::http::auth::scope_for;

/// 从 openapi.yaml 抓取 (方法, 路径) 对——只做最小行扫描，不引入 yaml 依赖。
fn openapi_routes() -> Vec<(String, String)> {
    let yaml = include_str!("../../docs/openapi.yaml");
    let methods = ["get", "post", "put", "delete", "patch"];
    let mut routes = Vec::new();
    let mut current_path: Option<String> = None;
    for line in yaml.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        if indent == 2 && trimmed.starts_with("/api/v1/") && trimmed.ends_with(':') {
            current_path = Some(trimmed.trim_end_matches(':').to_string());
            continue;
        }
        if indent == 4 && trimmed.ends_with(':') {
            let verb = trimmed.trim_end_matches(':');
            if methods.contains(&verb)
                && let Some(path) = &current_path
            {
                routes.push((verb.to_string(), path.clone()));
            }
        }
    }
    routes
}

/// 不参与 scope 编目的路径（JWT-only、公开，或自带 token 自查的 WS/cert 端点——
/// 后者不走 require_auth 中间件，scope 在 handler 内用 verify_scoped_token 强制）。
const EXEMPT_PATHS: &[&str] = &[
    "/api/v1/api-keys",
    "/api/v1/api-keys/{id}",
    "/api/v1/auth/login",
    "/api/v1/auth/me",
    "/api/v1/auth/change-password",
    "/api/v1/auth/change-username",
    "/api/v1/agents/cert",
    "/api/v1/mcp",
    "/api/v1/agents/{id}/terminal",
    "/api/v1/jobs/{id}/stream",
    "/api/v1/metrics/stream",
    "/api/v1/notifications/stream",
    "/api/v1/services/{id}/logs/stream",
    "/api/v1/ir/memscan/{id}/stream",
];

#[test]
fn every_openapi_v1_route_is_scoped_or_exempt() {
    let routes = openapi_routes();
    assert!(
        routes.len() >= 60,
        "openapi 解析异常，路由数过少: {}",
        routes.len()
    );
    let mut missing = Vec::new();
    for (_method, path) in &routes {
        if EXEMPT_PATHS.contains(&path.as_str()) {
            continue;
        }
        if scope_for(path).is_none() {
            missing.push(path.clone());
        }
    }
    assert!(
        missing.is_empty(),
        "以下路由未编目 scope（API key 将被 fail-closed 拒绝）: {missing:?}"
    );
}

#[test]
fn sensitive_routes_are_never_scoped() {
    // api-keys / 账号管理：任何情况下都不能对 API key 开放
    for path in [
        "/api/v1/api-keys",
        "/api/v1/api-keys/{id}",
        "/api/v1/auth/me",
        "/api/v1/auth/change-password",
        "/api/v1/auth/change-username",
        "/api/v1/auth/login",
    ] {
        assert!(scope_for(path).is_none(), "{path} 不应对 API key 编目");
    }
}

#[test]
fn core_domains_are_mapped() {
    use helm_server::application::scopes;
    for (path, want) in [
        ("/api/v1/hosts", scopes::HOSTS),
        ("/api/v1/exec", scopes::EXEC),
        ("/api/v1/files/upload", scopes::FILES),
        ("/api/v1/sys-services/action", scopes::SERVICES),
        ("/api/v1/processes/kill", scopes::PROCESSES),
        ("/api/v1/metrics", scopes::METRICS),
        ("/api/v1/notifications", scopes::NOTIFICATIONS),
        ("/api/v1/listeners", scopes::LISTENERS),
        ("/api/v1/forward/exec", scopes::FORWARD),
        ("/api/v1/proxies", scopes::PROXY),
        ("/api/v1/ir/scan", scopes::IR),
        ("/api/v1/agent-gen", scopes::AGENT_GEN),
        ("/api/v1/audit", scopes::AUDIT),
        ("/api/v1/skill", scopes::SKILL),
    ] {
        assert_eq!(scope_for(path), Some(want), "{path} 编目错误");
    }
}
