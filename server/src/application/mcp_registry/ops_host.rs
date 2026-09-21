//! host 域 op（28 条）：**对主机做的一切**（`host_id` / `agent_id` 为第一参数）。
//!
//! 自 `mcp_registry.rs` 拆出（G7：760 行单文件越界）。域内数据表独立成文、与 platform 域
//! 分开维护；两表由 [`super::ops`] 合成全量目录，顺序 = host 在前、platform 在后。
//!
//! ⚠ `scripts/gen_mcp_catalog.py` 按文本解析本文件（认 `// -- <域> 域` 注释与 `op!(`
//! 块），改动块结构需同步该脚本。

use super::{OpDef, Os, op};

pub static HOST_OPS: &[OpDef] = &[
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
];
