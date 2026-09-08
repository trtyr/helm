# Overview — helm 集中式运维平台

## 一句话

一个**集中式运维 + 应急响应平台**：把轻量 Agent 下发到目标主机（Windows / Linux / macOS），即可从控制端对整批主机做**远程命令执行、文件上下传、交互式终端、常驻服务管理、进程/网络管控、指标采集与告警、审计**，以及完整的**应急响应能力**（自启动项全景扫描与操作、基线快照对比、进程树+异常父子检测、流式内存扫描、安全事件审计、NTFS 文件时间线、批量操作、证据包收集、权限检测），并提供完整的 HTTP API 与 OpenAPI 契约供前端控制台消费。

## 两大组件

```text
[控制台 / HTTP API] ──► Server 控制端 ◄──gRPC 双向流（可选 mTLS）──► Agent 被控端（各目标主机）
```

- **Server（`server/`）**：集中管理、任务编排、状态收集、持久化（Postgres）、对外 HTTP API（含 WebSocket 实时流）。
- **Agent（`agent/`）**：装在目标机，执行命令、采集指标、文件传输、交互终端、常驻服务、进程/网络信息、
  IR 应急采集（自启动/日志/内存/文件时间线）与操作（禁用/启用/删除持久化）。单二进制、跨平台。
- **proto（`proto/`）**：gRPC 契约（protobuf），Server 与 Agent 的**单一事实来源**。

## 核心能力

| 能力 | 说明 |
|------|------|
| 命令执行 | 控制台下发命令 → Server 建 Job → 经双向流推送 → Agent 执行并流式回报 stdout/stderr + 退出码（Windows 下按 OEM 代码页解码，中文不乱码） |
| 文件传输 | upload（下发到目标机）/ download（从目标机取回）+ 列目录，分块 + sha256 校验和 |
| 指标采集 | Agent 每 30s 采集 CPU / 内存 / 磁盘 / 网络 / 进程数，上报落库（时间序列） |
| 定时任务 | 按间隔周期性下发命令，任务落库，**Server 重启后自动恢复调度** |
| 正向连接 | Agent 监听、Server 主动拨号建立**持久连接**（reconciler 差分启停 + 断线重连），注册后全端点可用；另保留按需拨号的 `forward/exec` |
| 监听器 | gRPC 监听器作为 DB 实体，可动态创建 / 启停 / 多实例 / 重启恢复 |
| 在线状态 | 心跳超时判定（阈值可配），`hosts` 端点暴露 online / last_seen / stale |
| Agent 生命周期 | 注销（删 agent + 孤儿主机软删）+ 卸载（SelfDestruct 自杀清二进制 + 自启） |
| 交互终端 | WebSocket + PTY 实时双向流（默认 shell，多开会话 + 空闲超时） |
| 服务管理 | 常驻后台任务：启动 / 停止 / 重启 / 日志 / 重启策略（restart_policy） |
| 进程 / 网络 | 进程列表 / kill + 网络接口信息采集 |
| 分组标签 | hosts.tags 设置 + 按标签过滤 |
| 安全 | Agent token 严格匹配 + 控制台 JWT（HS256）+ mTLS（反向：token 换证书；正向：管理员预置证书，CA 可持久化）+ 审计日志 |
| 监控告警 | 磁盘 / 网络指标 + 阈值告警落库 + 时序保留清理（30 天） |
| API 完整性 | 全实体 CRUD（DELETE/UPDATE）+ 分页/过滤 + agents 详情 |
| 实时流 | WebSocket：服务日志 tail-f / job 输出流 / metrics 指标流 / 通知流 |
| 通知 | 系统内小卡片（决策 009）：上线/下线/预警 + 已读未读 + WS 实时推送 + 5 分钟冷却合并，不做外发 |
| 契约 | `docs/openapi.yaml`（OpenAPI 3.0.3）覆盖全部 HTTP 端点，可机器校验 |
| **IR 应急响应** | 自启动项全景 12 分类（签名校验+厂商）+ 禁用/启用/删除（Autoruns 机制）+ 基线快照对比 + 异常父子检测 + 流式内存扫描（全进程/单 PID、SeDebugPrivilege）+ 安全事件审计 + NTFS USN 文件时间线 + VirusTotal 接口 + 证据包一键收集 |
| **批量操作** | 多主机批量命令下发（逐 agent 建 job，离线标 error） |
| **权限检测** | Windows Administrators 组 / Unix uid=0 自动检测，前端徽章显示 |
| **页面缓存** | 自启动项/系统日志扫描结果服务端缓存，页面秒开 |

## 连接模式

- **反向（默认）**：Agent 主动连 Server，穿透 NAT/防火墙。
- **正向**：Agent 监听 gRPC 端口，Server 主动拨号（同区域内网 / 公网被控端只监听不回连）。

两种模式复用**同一套信令协议**（gRPC 双向流 `OpenChannel` / `OpenForwardChannel`），
见 [architecture.md](architecture.md) 与 [api.md](api.md)。mTLS 取证路径不同：反向用
token 经 HTTP 换证书并缓存；正向由管理员在 Server 侧离线签发后预置（`--issue-cert`），
详见 [run-and-deploy.md](run-and-deploy.md)。

## 技术轮廓

- **语言/框架**：Rust（edition 2024），tokio 异步；axum（HTTP + WebSocket）+ tonic（gRPC，tls-ring）+ sqlx（Postgres）。
- **仓库形态**：Cargo workspace，三个成员 crate：`proto` / `server` / `agent`。
- **持久化**：PostgreSQL，sqlx 编译期嵌入迁移（14 个版本）。
- **安全**：rcgen 内置 CA + mTLS；bcrypt 密码哈希 + JWT。
- **前端控制台**：`console/`（Vite + React，单仓，M1–M4 已全落地——多路由：主机/概览/终端/文件/服务/进程/网络/自启动项/系统日志/内存扫描/代理/任务/
  通知/告警/审计/监听器/仪表盘等）；本仓库同仓提供 HTTP API + OpenAPI
  契约供其消费（设计规格见 docs/plantree/plans/frontend/）。

详见 [tech-stack.md](tech-stack.md)。
