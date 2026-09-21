//! platform 域 op（13 条）：**平台自身管理**（主机档案 / agent / 监听器 / 代理 / agent 生成）。
//!
//! 自 `mcp_registry.rs` 拆出（G7）。与 [`super::ops_host`] 分开维护，两者由 [`super::ops`]
//! 按「host 在前、platform 在后」合成全量目录——顺序即 `catalog` 的域顺序。
//!
//! ⚠ `scripts/gen_mcp_catalog.py` 按文本解析本文件（认 `// -- <域> 域` 注释与 `op!(`
//! 块），改动块结构需同步该脚本。

use super::{OpDef, Os, op};

pub static PLATFORM_OPS: &[OpDef] = &[
    // -- platform 域：平台自身管理 --
    op!(
        "hosts.list",
        "hosts",
        Os::Any,
        "GET",
        "/api/v1/hosts",
        "主机列表（含在线状态/系统版本/标签）",
        &[
            ("page", "页码，默认 1"),
            ("limit", "每页条数，默认 20"),
            ("tag", "按标签过滤（可选）")
        ]
    ),
    op!(
        "agent.tags",
        "hosts",
        Os::Any,
        "PUT",
        "/api/v1/agents/{id}/tags",
        "给 agent 打标签",
        &[("id", "agent_id"), ("tags", "标签数组")]
    ),
    op!(
        "agent.deregister",
        "hosts",
        Os::Any,
        "DELETE",
        "/api/v1/agents/{id}",
        "注销 agent（删档案；破坏性）",
        &[("id", "agent_id")]
    ),
    op!(
        "agent.uninstall",
        "hosts",
        Os::Any,
        "POST",
        "/api/v1/agents/{id}/uninstall",
        "卸载 agent（下发自毁指令；破坏性）",
        &[
            ("id", "agent_id"),
            ("remove_binary", "是否删除目标机上的二进制，默认 false")
        ]
    ),
    op!(
        "listeners.list",
        "listeners",
        Os::Any,
        "GET",
        "/api/v1/listeners",
        "gRPC 监听器列表",
        &[]
    ),
    op!(
        "listeners.create",
        "listeners",
        Os::Any,
        "POST",
        "/api/v1/listeners",
        "创建 gRPC 监听器",
        &[
            ("name", "名称"),
            ("addr", "监听地址（如 0.0.0.0:50051）"),
            ("proto", "协议，grpc")
        ]
    ),
    op!(
        "listeners.action",
        "listeners",
        Os::Any,
        "POST",
        "/api/v1/listeners/{id}/{action}",
        "监听器 start/stop",
        &[("id", "监听器 uuid"), ("action", "start|stop")]
    ),
    op!(
        "listeners.delete",
        "listeners",
        Os::Any,
        "DELETE",
        "/api/v1/listeners/{id}",
        "删除监听器（破坏性）",
        &[("id", "监听器 uuid")]
    ),
    op!(
        "proxies.list",
        "proxy",
        Os::Any,
        "GET",
        "/api/v1/proxies",
        "活跃 SOCKS5 代理列表",
        &[]
    ),
    op!(
        "proxies.create",
        "proxy",
        Os::Any,
        "POST",
        "/api/v1/proxies",
        "为 agent 开 SOCKS5 代理（Server 本地监听，流量经 agent 出站）",
        &[
            ("agent_id", "agent_id"),
            ("listen_addr", "Server 侧监听地址，默认 127.0.0.1:1080")
        ]
    ),
    op!(
        "proxies.stop",
        "proxy",
        Os::Any,
        "DELETE",
        "/api/v1/proxies/{id}",
        "停止代理",
        &[("id", "代理 uuid")]
    ),
    op!(
        "agent_gen.create",
        "agent-gen",
        Os::Any,
        "POST",
        "/api/v1/agent-gen",
        "创建 agent 现场编译任务（异步，用 agent_gen.status 轮询）",
        &[
            ("os", "windows|linux|macos"),
            ("arch", "x86_64|aarch64"),
            ("listener_id", "监听器 uuid"),
            ("conn_mode", "reverse|forward"),
            ("server_addr", "烙入地址（可选）")
        ]
    ),
    op!(
        "agent_gen.status",
        "agent-gen",
        Os::Any,
        "GET",
        "/api/v1/agent-gen/{id}",
        "查询编译任务状态与日志尾",
        &[("id", "任务 uuid")]
    ),
];
