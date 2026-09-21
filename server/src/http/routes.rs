//! `/api/v1` 受保护路由的**分组构造**（G7 拆分，2026-09-20）。
//!
//! 按业务面分组，每组一个 `Router<AppState>`；由父模块的 [`super::protected_routes`] 合并
//! 并统一挂 JWT/API-key 中间件。分组只为可读与可增量维护——**路径与 handler 绑定必须与
//! 拆分前逐条一致**（openapi 对账由 `scripts/check_openapi.py` 兜底）。
//!
//! 各组对本模块 **pub(super)**。

use super::AppState;
use crate::http::{
    agent_gen, agents, alerts, api_keys, audit, auth, exec, files, forward, hosts, ir, ir_ops,
    jobs, listeners, logs, metrics, notifications, p2, process, proxies, services, tasks,
};
use axum::Router;
use axum::routing::{delete, get, post, put};

/// 主机、Agent 与其生成任务。
pub(super) fn host_routes() -> Router<AppState> {
    Router::new()
        .route("/hosts", get(hosts::list_hosts).post(hosts::create_host))
        .route(
            "/hosts/{id}",
            get(hosts::get_host)
                .put(hosts::update_host)
                .delete(hosts::delete_host),
        )
        .route("/hosts/{id}/tags", post(hosts::set_host_tags))
        .route("/agents", get(agents::list_agents))
        .route(
            "/agents/{id}",
            get(agents::get_agent).delete(agents::deregister_agent),
        )
        .route("/agents/{id}/tags", put(agents::update_agent_tags))
        .route("/agents/{id}/uninstall", post(agents::uninstall_agent))
        .route(
            "/agent-gen",
            get(agent_gen::list_jobs).post(agent_gen::create),
        )
        .route("/agent-gen/{id}", get(agent_gen::get_job))
        .route("/agent-gen/{id}/download", get(agent_gen::download))
}

/// 执行面：命令、任务、文件、正向执行。
pub(super) fn exec_routes() -> Router<AppState> {
    Router::new()
        .route("/exec", post(exec::exec))
        .route("/exec/batch", post(p2::batch_exec))
        .route("/jobs", get(jobs::list_jobs))
        .route("/jobs/{id}", get(jobs::get_job))
        .route("/jobs/{id}/cancel", post(jobs::cancel_job))
        .route("/logs/events", get(logs::list_events))
        .route("/tasks/script", post(tasks::run_script))
        .route("/tasks/schedule", post(tasks::schedule))
        .route("/forward/exec", post(forward::exec))
        .route("/files/upload", post(files::upload))
        .route("/files/download", post(files::download))
        .route("/files/list", post(files::list))
}

/// 监听器、托管服务、代理与进程/网络查询。
pub(super) fn service_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/listeners",
            get(listeners::list_listeners).post(listeners::create_listener),
        )
        .route("/listeners/{id}/start", post(listeners::start_listener))
        .route("/listeners/{id}/stop", post(listeners::stop_listener))
        .route(
            "/listeners/{id}",
            put(listeners::update_listener).delete(listeners::delete_listener),
        )
        .route(
            "/services",
            get(services::list_services).post(services::create_service),
        )
        .route("/services/{id}/start", post(services::start_service))
        .route("/services/{id}/stop", post(services::stop_service))
        .route("/services/{id}/restart", post(services::restart_service))
        .route("/services/{id}/logs", get(services::service_logs))
        .route(
            "/services/{id}",
            put(services::update_service).delete(services::delete_service),
        )
        .route(
            "/proxies",
            get(proxies::list_proxies).post(proxies::create_proxy),
        )
        .route("/proxies/{id}", delete(proxies::stop_proxy))
        .route("/processes/list", post(process::list_processes))
        .route("/processes/kill", post(process::kill_process))
        .route("/net/info", post(process::net_info))
        .route("/sys-services/list", post(process::list_sys_services))
        .route("/sys-services/action", post(process::sys_service_action))
}

/// 应急响应（IR）面。
pub(super) fn ir_routes() -> Router<AppState> {
    Router::new()
        .route("/ir/scan", post(ir::ir_scan))
        .route("/ir/memscan", post(ir::mem_scan))
        .route("/ir/memscan/stream", post(ir_ops::memscan_stream_start))
        .route("/ir/cache", get(ir::get_cache))
        .route("/ir/fs-timeline", post(p2::fs_timeline))
        .route("/ir/evidence", post(p2::evidence))
        .route("/ir/autorun-action", post(ir_ops::autorun_action))
        .route(
            "/ir/snapshots",
            post(ir_ops::create_snapshot).get(ir_ops::list_snapshots),
        )
        .route("/ir/snapshots/compare", post(ir_ops::compare_snapshots))
        .route(
            "/ir/snapshots/{id}",
            get(ir_ops::get_snapshot).delete(ir_ops::delete_snapshot),
        )
}

/// 指标、审计、告警、通知、API key 与账号自助。
pub(super) fn meta_routes() -> Router<AppState> {
    Router::new()
        .route("/metrics", get(metrics::list_metrics))
        .route("/audit", get(audit::list_audit))
        .route("/alerts", get(alerts::list_alerts))
        .route("/notifications", get(notifications::list_notifications))
        .route(
            "/notifications/unread-count",
            get(notifications::unread_count),
        )
        .route("/notifications/{id}/read", post(notifications::mark_read))
        .route(
            "/notifications/read-all",
            post(notifications::mark_all_read),
        )
        .route("/api-keys", get(api_keys::list).post(api_keys::create))
        .route(
            "/api-keys/{id}",
            get(api_keys::get_one).delete(api_keys::revoke),
        )
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/auth/change-username", post(auth::change_username))
}
