//! MCP 操作注册表：单工具 `helm` 的 op 目录（渐进式编目，见 docs/mcp.md）。
//!
//! 每个 op 是对平台既有 HTTP 端点的声明式映射（method + path 模板 + 参数说明），
//! 由 http/mcp.rs 翻译成 loopback 自调用——MCP 面 == HTTP 面，鉴权/审计零重复。
//! AI 看到的目录按其 API key 的 scope 裁剪、按 os 分组，实现渐进式发现。

use serde_json::{Value, json};

/// op 适用的操作系统维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Any,
    Windows,
    Linux,
}

impl Os {
    pub fn as_str(self) -> &'static str {
        match self {
            Os::Any => "any",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    /// 解析调用方传入的 os 参数。
    pub fn parse(s: &str) -> Option<Os> {
        match s {
            "windows" => Some(Os::Windows),
            "linux" => Some(Os::Linux),
            _ => None,
        }
    }
}

/// 一个可被 AI 调用的操作。
pub struct OpDef {
    /// 操作名（点分域前缀，如 "hosts.list"）。
    pub name: &'static str,
    /// 所需 API key scope（与 application::scopes 对应）。
    pub scope: &'static str,
    pub os: Os,
    pub method: &'static str,
    /// HTTP 路径模板，`{x}` 占位符从 args 取值。
    pub path: &'static str,
    /// 中文一句话说明（工具索引与编目用）。
    pub summary: &'static str,
    /// 参数说明（args 的键 → 中文说明；path 占位符也在此声明）。
    pub params: &'static [(&'static str, &'static str)],
}

macro_rules! op {
    ($name:literal, $scope:literal, $os:expr, $method:literal, $path:literal, $summary:literal, $params:expr) => {
        OpDef {
            name: $name,
            scope: $scope,
            os: $os,
            method: $method,
            path: $path,
            summary: $summary,
            params: $params,
        }
    };
}

/// 全量操作注册表（按域分组）。新增能力 = 加一条 + 在 http::auth::scope_for 编目。
pub static OPS: &[OpDef] = &[
    // -- host 域：对主机做的一切（host_id/agent_id 为第一参数）--
    op!(
        "exec.run",
        "exec",
        Os::Any,
        "POST",
        "/api/v1/exec",
        "在目标机执行命令（异步 job，返回 job_id 后用 jobs.get 取结果）",
        &[
            ("agent_id", "agent_id"),
            ("command", "命令"),
            ("args", "参数数组（可选）")
        ]
    ),
    op!(
        "exec.batch",
        "exec",
        Os::Any,
        "POST",
        "/api/v1/exec/batch",
        "多主机批量执行同一命令",
        &[
            ("agent_ids", "agent_id 数组"),
            ("command", "命令"),
            ("args", "参数数组（可选）")
        ]
    ),
    op!(
        "jobs.get",
        "exec",
        Os::Any,
        "GET",
        "/api/v1/jobs/{id}",
        "查询 job 状态与输出",
        &[("id", "job_id")]
    ),
    op!(
        "jobs.list",
        "exec",
        Os::Any,
        "GET",
        "/api/v1/jobs",
        "job 列表",
        &[("page", "页码（可选）"), ("limit", "条数（可选）")]
    ),
    op!(
        "tasks.script",
        "exec",
        Os::Any,
        "POST",
        "/api/v1/tasks/script",
        "跑一段脚本（--sh 或 --ps 封装）",
        &[
            ("agent_id", "agent_id"),
            ("command", "解释器（如 sh/bash/powershell）"),
            ("args", "参数数组")
        ]
    ),
    op!(
        "tasks.schedule",
        "exec",
        Os::Any,
        "POST",
        "/api/v1/tasks/schedule",
        "创建定时任务（按间隔重复执行）",
        &[
            ("agent_id", "agent_id"),
            ("command", "命令"),
            ("args", "参数数组"),
            ("interval_secs", "间隔秒数")
        ]
    ),
    op!(
        "forward.exec",
        "forward",
        Os::Any,
        "POST",
        "/api/v1/forward/exec",
        "正向连接执行（Server 主动拨号 agent）",
        &[
            ("hostname", "主机名或 agent_addr"),
            ("command", "命令"),
            ("args", "参数数组（可选）")
        ]
    ),
    op!(
        "files.upload",
        "files",
        Os::Any,
        "POST",
        "/api/v1/files/upload",
        "从 Server 侧上传文件到目标机",
        &[
            ("agent_id", "agent_id"),
            ("local_path", "Server 上的源路径"),
            ("remote_path", "目标机路径")
        ]
    ),
    op!(
        "files.download",
        "files",
        Os::Any,
        "POST",
        "/api/v1/files/download",
        "从目标机下载文件到 Server 侧",
        &[
            ("agent_id", "agent_id"),
            ("remote_path", "目标机路径"),
            ("local_path", "Server 上的落路径")
        ]
    ),
    op!(
        "files.list",
        "files",
        Os::Any,
        "POST",
        "/api/v1/files/list",
        "列目标机目录",
        &[("agent_id", "agent_id"), ("path", "目录路径")]
    ),
    op!(
        "sys_services.list",
        "services",
        Os::Any,
        "POST",
        "/api/v1/sys-services/list",
        "系统服务发现（Windows SCM / systemd / launchctl）",
        &[("agent_id", "agent_id")]
    ),
    op!(
        "sys_services.action",
        "services",
        Os::Any,
        "POST",
        "/api/v1/sys-services/action",
        "系统服务 start/stop/restart（需权限）",
        &[
            ("agent_id", "agent_id"),
            ("name", "服务名/unit 名"),
            ("action", "start|stop|restart")
        ]
    ),
    op!(
        "services.list",
        "services",
        Os::Any,
        "GET",
        "/api/v1/services",
        "平台托管常驻服务列表",
        &[]
    ),
    op!(
        "services.create",
        "services",
        Os::Any,
        "POST",
        "/api/v1/services",
        "创建平台托管常驻服务",
        &[
            ("agent_id", "agent_id"),
            ("name", "服务名"),
            ("command", "命令"),
            ("args", "参数（可选）"),
            ("restart_policy", "重启策略（可选）")
        ]
    ),
    op!(
        "services.action",
        "services",
        Os::Any,
        "POST",
        "/api/v1/services/{id}/{action}",
        "托管服务 start/stop/restart",
        &[("id", "服务 uuid"), ("action", "start|stop|restart")]
    ),
    op!(
        "services.logs",
        "services",
        Os::Any,
        "GET",
        "/api/v1/services/{id}/logs",
        "托管服务日志快照",
        &[("id", "服务 uuid")]
    ),
    op!(
        "services.delete",
        "services",
        Os::Any,
        "DELETE",
        "/api/v1/services/{id}",
        "删除托管服务（破坏性）",
        &[("id", "服务 uuid")]
    ),
    op!(
        "processes.list",
        "processes",
        Os::Any,
        "POST",
        "/api/v1/processes/list",
        "进程列表（Linux 含内核线程）",
        &[("agent_id", "agent_id")]
    ),
    op!(
        "processes.kill",
        "processes",
        Os::Any,
        "POST",
        "/api/v1/processes/kill",
        "结束进程（破坏性）",
        &[("agent_id", "agent_id"), ("pid", "进程号")]
    ),
    op!(
        "net.info",
        "processes",
        Os::Any,
        "POST",
        "/api/v1/net/info",
        "网络信息（网卡 + 连接表）",
        &[("agent_id", "agent_id")]
    ),
    op!(
        "ir.scan",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/scan",
        "自启动项全景扫描（Autoruns 12 分类 + 签名校验）",
        &[
            ("agent_id", "agent_id"),
            ("types", "扫描类型数组（可选，空=全部）")
        ]
    ),
    op!(
        "ir.autorun_action",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/autorun-action",
        "自启动项 禁用/启用/删除（破坏性）",
        &[
            ("agent_id", "agent_id"),
            ("action", "disable|enable|delete"),
            ("key", "op_key（来自 ir.scan 结果）")
        ]
    ),
    op!(
        "ir.memscan_start",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/memscan/stream",
        "启动流式内存扫描任务",
        &[
            ("agent_id", "agent_id"),
            ("pid", "进程号，0=全进程"),
            ("keyword", "匹配字符串"),
            ("min_len", "最小长度（可选）")
        ]
    ),
    op!(
        "ir.fs_timeline",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/fs-timeline",
        "NTFS USN 文件活动时间线",
        &[
            ("agent_id", "agent_id"),
            ("drive", "盘符（如 C）"),
            ("since_hours", "近 N 小时（可选）"),
            ("keyword", "文件名过滤（可选）")
        ]
    ),
    op!(
        "ir.evidence",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/evidence",
        "一键证据包（进程/网络/服务/自启动/日志 JSON）",
        &[("agent_id", "agent_id")]
    ),
    op!(
        "ir.snapshots_save",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/snapshots",
        "保存自启动项基线快照",
        &[("agent_id", "agent_id"), ("name", "快照名")]
    ),
    op!(
        "ir.snapshots_list",
        "ir",
        Os::Windows,
        "GET",
        "/api/v1/ir/snapshots",
        "快照列表",
        &[]
    ),
    op!(
        "ir.snapshots_compare",
        "ir",
        Os::Windows,
        "POST",
        "/api/v1/ir/snapshots/compare",
        "与基线快照对比，diff 出新增/移除项",
        &[("id", "基线快照 uuid"), ("agent_id", "agent_id")]
    ),
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

/// 调用方可见的 op：scope 匹配（空 scopes = 全部）+ os 匹配（可选过滤）。
pub fn allowed_ops(scopes: &[String], os: Option<Os>) -> Vec<&'static OpDef> {
    OPS.iter()
        .filter(|op| {
            let scope_ok = scopes.is_empty() || scopes.iter().any(|s| s == op.scope);
            let os_ok = match os {
                None => true,
                Some(filter) => op.os == Os::Any || op.os == filter,
            };
            scope_ok && os_ok
        })
        .collect()
}

/// 按名称找 op。
pub fn find(name: &str) -> Option<&'static OpDef> {
    OPS.iter().find(|op| op.name == name)
}

/// 从路径模板提取占位符名（如 "/api/v1/jobs/{id}" → ["id"]）。
pub fn placeholders(path: &str) -> Vec<&str> {
    path.split('{')
        .skip(1)
        .filter_map(|seg| seg.split('}').next())
        .collect()
}

/// 工具描述里的紧凑 op 索引（scope 裁剪后，按 os 分组）。
/// 工具描述（P002 T4 分层）：tier 1 不展开清单（catalog 发现）/
/// tier 2 仅 host 域（对主机做的一切，ir 带 windows 专属标注）/ tier 3 全量。
pub fn tool_description(scopes: &[String], tier: u8) -> String {
    let ops: Vec<&OpDef> = allowed_ops(scopes, None)
        .into_iter()
        .filter(|o| match tier {
            1 => false,
            2 => top_group(o.name) == "host",
            _ => true,
        })
        .collect();
    let mut text = String::from(
        "helm 集中式运维平台操作工具。可管理主机/agent、执行命令、传输文件、\
         管控服务与进程、应急响应（IR，仅 Windows）等。\n\
         渐进式用法：先调 {\"op\":\"catalog\"} 获取完整参数说明；\
         每次调用结果尾部附 available_ops 提示。\n\n",
    );
    if ops.is_empty() {
        text.push_str("当前分层未展开操作清单；调用 {\"op\":\"catalog\"} 查看可用操作。\n");
        return text;
    }
    text.push_str("可用操作：\n");
    for group in [Os::Any, Os::Windows, Os::Linux] {
        let group_ops: Vec<&OpDef> = ops.iter().copied().filter(|o| o.os == group).collect();
        if group_ops.is_empty() {
            continue;
        }
        if group != Os::Any {
            text.push_str(&format!("\n[{} 专属]\n", group.as_str()));
        }
        for op in group_ops {
            text.push_str(&format!("{} — {}\n", op.name, op.summary));
        }
    }
    text
}

/// catalog op 的完整编目（scope 裁剪 + 可选 domain/os 过滤）。
/// op 所属组（P002 两域极简：host = 对主机做的一切 / platform = 平台自身管理）。
/// platform 显式清单，其余默认 host（新增 op 的默认语义 = 操作目标主机）。
fn top_group(name: &str) -> &'static str {
    match name.split('.').next().unwrap_or("") {
        "hosts" | "agent" | "listeners" | "proxies" | "agent_gen" => "platform",
        _ => "host",
    }
}

pub fn catalog_json(scopes: &[String], domain: Option<&str>, os: Option<Os>, tier: u8) -> Value {
    let mut by_group: Vec<(String, Vec<Value>)> = Vec::new();
    for op in allowed_ops(scopes, os) {
        if let Some(d) = domain {
            let prefix = format!("{d}.");
            if !op.name.starts_with(&prefix) && op.name != d {
                continue;
            }
        }
        let params: Value = op
            .params
            .iter()
            .map(|(k, v)| json!({ "name": k, "desc": v }))
            .collect();
        let entry = json!({
            "op": op.name,
            "scope": op.scope,
            "os": op.os.as_str(),
            "method": op.method,
            "path": op.path,
            "summary": op.summary,
            "params": params,
        });
        let group = top_group(op.name);
        match by_group.iter_mut().find(|(g, _)| *g == group) {
            Some((_, list)) => list.push(entry),
            None => by_group.push((group.to_string(), vec![entry])),
        }
    }
    let domains: Value = by_group
        .into_iter()
        .map(|(group, ops)| json!({ "group": group, "ops": ops }))
        .collect();
    json!({
        "tool": "helm",
        "total": allowed_ops(scopes, os).len(),
        "tier": tier,
        "domains": domains,
        "hint": "调用方式：{\"op\": \"<操作名>\", \"os\": \"linux\"|\"windows\"（OS 专属操作必填）, \"args\": {...}}；路径占位符（如 {id}）直接放在 args 里"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_names_are_unique_and_dot_prefixed() {
        let mut names: Vec<_> = OPS.iter().map(|o| o.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "op 名重复");
        assert!(
            OPS.iter()
                .all(|o| o.name.split('.').next().unwrap_or("").len() > 1),
            "op 名必须带域前缀"
        );
    }

    #[test]
    fn every_op_scope_is_valid() {
        for op in OPS {
            assert!(
                crate::application::scopes::is_valid(op.scope),
                "op {} 引用了未定义 scope {}",
                op.name,
                op.scope
            );
        }
    }

    #[test]
    fn placeholder_extraction() {
        assert_eq!(placeholders("/api/v1/jobs/{id}"), vec!["id"]);
        assert_eq!(
            placeholders("/api/v1/services/{id}/{action}"),
            vec!["id", "action"]
        );
        assert!(placeholders("/api/v1/hosts").is_empty());
    }

    #[test]
    fn scope_filtering_hides_ir_without_scope() {
        let full = allowed_ops(&[], None);
        let restricted = allowed_ops(&["exec".to_string()], None);
        assert!(full.iter().any(|o| o.name == "ir.scan"));
        assert!(!restricted.iter().any(|o| o.name == "ir.scan"));
        assert!(restricted.iter().any(|o| o.name == "exec.run"));
        assert!(
            !tool_description(&["exec".to_string()], 3).contains("ir.scan"),
            "受限 key 的工具描述不应出现未授权 op"
        );
    }

    #[test]
    fn tool_description_respects_tier() {
        // tier 1：清单不展开，catalog 发现提示仍在
        let t1 = tool_description(&[], 1);
        assert!(!t1.contains("exec.run"), "tier 1 不应列 op 清单");
        assert!(t1.contains("catalog"), "tier 1 仍应提示 catalog 发现");

        // tier 2：host 域全量（ir 即暴露且带 windows 标注），platform 域不出现
        let t2 = tool_description(&[], 2);
        assert!(t2.contains("exec.run"));
        assert!(
            t2.contains("ir.scan"),
            "ir 应在 tier 2 即暴露（Windows 主机语义，owner 拍板）"
        );
        assert!(
            t2.contains("windows 专属"),
            "ir 应带 windows 专属标注（暴露但警示，Q2 倾向）"
        );
        assert!(
            !t2.contains("listeners.create"),
            "tier 2 不应含 platform 域 op"
        );

        // tier 3：全量（含 platform 管理能力）
        let t3 = tool_description(&[], 3);
        assert!(t3.contains("exec.run") && t3.contains("listeners.create"));
    }

    #[test]
    fn catalog_respects_os_filter() {
        // 当前注册表无 Linux 专属 op：Linux 过滤 = Any 全保留 + Windows 全剔除
        let catalog = catalog_json(&[], None, Some(Os::Linux), 3).to_string();
        assert!(
            !catalog.contains("\"os\":\"windows\""),
            "Linux 过滤后不应含 windows op"
        );
        assert!(catalog.contains("\"os\":\"any\""));
    }
}
