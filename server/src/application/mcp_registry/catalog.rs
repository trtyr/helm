//! op 目录的**查询与编目逻辑**：scope/os 裁剪、按名查找、占位符提取、工具描述分层、
//! 三级目录 JSON。
//!
//! 自 `mcp_registry.rs` 拆出（G7：760 行单文件越界）——数据表在 [`super::ops_host`] /
//! [`super::ops_platform`]，本文件只放逻辑。

use super::{OpDef, Os, ops};
use serde_json::{Value, json};

/// 调用方可见的 op：scope 匹配（空 scopes = 全部）+ os 匹配（可选过滤）。
pub fn allowed_ops(scopes: &[String], os: Option<Os>) -> Vec<&'static OpDef> {
    ops()
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
    ops().find(|op| op.name == name)
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

/// op 所属能力组（P002 复验反馈：目录三级化——域 → 能力组 → 工具，能力聚合 8 组）。
fn top_subgroup(name: &str) -> &'static str {
    match name.split('.').next().unwrap_or("") {
        "exec" | "jobs" | "tasks" => "执行",
        "files" => "文件",
        "services" | "sys_services" | "processes" | "net" => "系统状态",
        "ir" => "取证 IR",
        "forward" => "反向执行",
        "hosts" | "agent" => "主机管理",
        "listeners" | "proxies" => "网络服务",
        "agent_gen" => "Agent 生成",
        _ => "其他",
    }
}

/// 三级目录的分组累积结构：域 → 能力组 → 工具条目列表。
type GroupedCatalog = Vec<(String, Vec<(String, Vec<Value>)>)>;

pub fn catalog_json(scopes: &[String], domain: Option<&str>, os: Option<Os>, tier: u8) -> Value {
    // 三级目录：域（host/platform）→ 能力组 → 工具（P002 复验反馈，能力聚合 8 组）。
    let mut by_group: GroupedCatalog = Vec::new();
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
        let subgroup = top_subgroup(op.name);
        let group_entry = match by_group.iter_mut().find(|(g, _)| *g == group) {
            Some(g) => g,
            None => {
                by_group.push((group.to_string(), Vec::new()));
                by_group.last_mut().expect("group just pushed")
            }
        };
        match group_entry.1.iter_mut().find(|(n, _)| *n == subgroup) {
            Some((_, list)) => list.push(entry),
            None => group_entry.1.push((subgroup.to_string(), vec![entry])),
        }
    }
    let domains: Value = by_group
        .into_iter()
        .map(|(group, subgroups)| {
            json!({
                "group": group,
                "subgroups": subgroups
                    .into_iter()
                    .map(|(name, ops)| json!({ "name": name, "ops": ops }))
                    .collect::<Vec<_>>(),
            })
        })
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
    fn top_group_splits_host_and_platform() {
        // platform 是显式清单，其余默认 host（新增 op 的默认语义 = 操作目标主机）
        assert_eq!(top_group("exec.run"), "host");
        assert_eq!(top_group("ir.scan"), "host");
        assert_eq!(top_group("files.list"), "host");
        assert_eq!(top_group("hosts.list"), "platform");
        assert_eq!(top_group("agent.tags"), "platform");
        assert_eq!(top_group("listeners.create"), "platform");
        assert_eq!(top_group("proxies.list"), "platform");
        assert_eq!(top_group("agent_gen.create"), "platform");
    }

    #[test]
    fn top_subgroup_covers_eight_groups() {
        assert_eq!(top_subgroup("exec.run"), "执行");
        assert_eq!(top_subgroup("jobs.list"), "执行");
        assert_eq!(top_subgroup("tasks.script"), "执行");
        assert_eq!(top_subgroup("files.list"), "文件");
        assert_eq!(top_subgroup("services.list"), "系统状态");
        assert_eq!(top_subgroup("ir.scan"), "取证 IR");
        assert_eq!(top_subgroup("forward.exec"), "反向执行");
        assert_eq!(top_subgroup("hosts.list"), "主机管理");
        assert_eq!(top_subgroup("listeners.list"), "网络服务");
        assert_eq!(top_subgroup("agent_gen.create"), "Agent 生成");
    }

    #[test]
    fn unknown_prefix_falls_back() {
        assert_eq!(top_group("weird.thing"), "host", "未知前缀按 host 兜底");
        assert_eq!(top_subgroup("weird.thing"), "其他");
    }

    #[test]
    fn every_registered_op_is_classified() {
        // 不变量：目录里每个 op 都必须落在某个明确的能力组（「其他」只能出现在未注册前缀上）
        for op in ops() {
            assert_ne!(
                top_subgroup(op.name),
                "其他",
                "op {} 未归入任何能力组（top_subgroup 漏了前缀？）",
                op.name
            );
        }
    }
}
