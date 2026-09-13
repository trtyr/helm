# helm MCP：把平台交给 AI

helm server 内嵌 MCP（Model Context Protocol）端点，面向任意标准 MCP 客户端
（Claude / Codex / 其他）。设计原则：

1. **渐进式单工具**：只暴露一个工具 `helm`，AI 通过 `op` 参数逐步发现、逐步调用，
   不一次性摊开几十个工具。工具描述内嵌 op 索引，`catalog` op 给出完整参数，
   每次调用结果尾部附 `available_ops` 提示。
2. **细分凭证**：AI 能做什么完全由其 API key 的 **scope** 决定——签发时按功能域勾选。
   MCP 面 == HTTP 面（tools/call 是对既有路由的 loopback 透传），鉴权/审计/错误映射零重复。

## 接入方式

```bash
# 签发一把带 scope 的 key（控制台：设置 → API 凭证；或 API 创建）
curl -X POST http://<server>:18081/api/v1/api-keys \
  -H "Authorization: Bearer <JWT>" -H "Content-Type: application/json" \
  -d '{"name": "ai-ops", "scopes": ["exec", "metrics", "services"]}'

# MCP 客户端配置（以 Claude Code 为例）
claude mcp add helm --transport http http://<server>:18081/mcp \
  --header "Authorization: Bearer helm_xxxxxxxx..."
```

MCP 端点：`POST /mcp`，Bearer 认证（API key 或 JWT），JSON-RPC 2.0，
支持 `initialize` / `tools/list` / `tools/call` / `ping` 与 batch 数组，无状态。

## Scope 清单

| Scope | 覆盖能力 |
|-------|---------|
| `hosts` | 主机/agent 档案、标签、注销、卸载 |
| `exec` | 命令执行、批量执行、脚本、定时任务、jobs、交互终端 |
| `files` | 文件上下传、目录列表 |
| `services` | 常驻服务、系统服务发现与启停（SCM/systemd） |
| `processes` | 进程列表/kill、网络信息 |
| `metrics` | 指标查询、告警 |
| `notifications` | 通知查看/已读 |
| `listeners` | gRPC 监听器管理 |
| `forward` | 正向连接执行 |
| `proxy` | SOCKS5 代理 |
| `ir` | 应急响应（自启动/内存扫描/时间线/证据包，Windows 目标机） |
| `agent-gen` | Agent 现场编译与下载 |
| `audit` | 审计日志读取 |
| `skill` | 运维技能包分发 |

规则：

- **空 scopes = 全功能**（兼容存量 key）。
- **api-keys 管理与账号端点没有 scope**，永远 JWT-only——AI 凭证不可能自签发、自扩权。
- fail-closed：新增受保护路由若未在 `http::auth::scope_for` 编目，API key 一律 403
  （`scope_map_test.rs` 用 openapi 契约对照防漏编）。

## 工具调用示例

```json
{"op": "catalog"}                                     // 本 key 可用的全部操作
{"op": "hosts.list"}                                  // 主机列表
{"op": "exec.run", "args": {"agent_id": "local-dev", "command": "uname", "args": ["-a"]}}
{"op": "jobs.get", "args": {"id": "<job_uuid>"}}      // 取执行结果
{"op": "sys_services.action", "args": {"agent_id": "x", "name": "nginx", "action": "restart"}}
{"op": "ir.scan", "os": "windows", "args": {"agent_id": "x"}}   // IR 仅 Windows
```

语义：

- 路径占位符（`{id}` 等）直接作为 `args` 的键。
- OS 专属 op（如 ir.*）在 `os` 不匹配时明确报错并列出该 OS 可用操作。
- 平台 4xx/5xx → MCP `isError: true` + 中文错误（403 附"请管理员签发 scope"指引）。
- 每次调用经既有路由 → 自动落审计（actor = `api-key:<名称>`），
  高危操作（kill/服务操作/autorun 变更/批量执行等）均有审计记录。

## 实现位置

- `server/src/http/mcp.rs` — JSON-RPC 端点与 loopback 透传
- `server/src/application/mcp_registry.rs` — op 注册表（40+ 操作）、编目、scope/os 过滤
- `server/src/application/scopes.rs` — scope 常量单一事实源
- `server/src/http/auth.rs` — `scope_for` 路由编目表 + 中间件强制（fail-closed）
- `scripts/e2e-mcp.py` — 端到端验证（签发→握手→裁剪→透传→越权→吊销）
