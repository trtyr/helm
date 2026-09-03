# Architecture — 模块地图与依赖方向

## 目录树

```text
helm/
├── Cargo.toml            # workspace（members: proto / server / agent）
├── Justfile              # 开发命令入口（fmt/clippy/test/buf/run）
├── buf.yaml              # protobuf lint + breaking 检查配置
├── docker-compose.yml    # 本地 Postgres（5433→5432）
├── proto/                # 共享 protobuf 契约（gRPC 底座）
│   ├── build.rs          # tonic-prost-build 编译 proto
│   ├── helm/agent/v1/    # agent.proto + types.proto
│   └── src/lib.rs        # pub mod pb { include!(OUT_DIR/...rs) }
├── server/               # Server 控制端
│   ├── src/
│   │   ├── main.rs / lib.rs      # 入口 + run() 装配
│   │   ├── config/               # clap 配置（env 覆盖）
│   │   ├── domain/               # 领域层：实体、状态机、类型化错误
│   │   ├── application/          # 应用层：用例编排（唯一业务入口）
│   │   ├── grpc/                 # gRPC 适配层（Agent 反向连入）
│   │   ├── http/                 # HTTP API 适配层（控制台）
│   │   ├── store/                # 持久化适配层（sqlx + 各实体仓储）
│   │   └── telemetry/            # tracing 初始化
│   ├── migrations/               # sqlx 迁移（7 个版本）
│   └── tests/                    # 集成测试（连真实 Postgres）
├── agent/                # Agent 被控端（单二进制、跨平台）
│   └── src/
│       ├── main.rs               # 入口：按 conn_mode 分派
│       ├── config/               # clap 配置
│       ├── cert.rs               # mTLS 证书：反向 token 换证书 + 缓存；正向预置加载
│       ├── connection.rs         # 反向：拨号 Server + 双向流 + 心跳 + 重连
│       ├── forward.rs            # 正向：gRPC server 监听（可选 mTLS）
│       ├── encoding.rs           # 控制台输出解码（Windows OEM 代码页 → UTF-8）
│       ├── exec.rs               # 命令执行并回报（输出经 encoding 解码）
│       ├── file.rs               # 文件传输（upload 会话 + download）
│       ├── fs.rs                 # 目录浏览（list_dir）
│       ├── monitor.rs            # 指标采集（sysinfo）
│       ├── process.rs            # 进程列表 / kill + 网络信息
│       ├── pty.rs                # 交互终端（portable-pty）
│       ├── service.rs            # 常驻服务管理（ServiceManager）
│       ├── uninstall.rs          # 自杀卸载（SelfDestruct）
│       └── telemetry/            # tracing 初始化（含按天滚动落文件）
├── deploy/               # 部署模板（systemd unit + Windows nssm 脚本）
├── scripts/              # e2e 脚本（Python）+ OpenAPI 校验
└── docs/                 # 本文档归档 + openapi.yaml + plantree 规划树
```

## Server 分层（六边形 / clean architecture）

依赖方向：**外层适配器 → 内层领域**。领域不 import 任何 IO 框架。

```text
        ┌─────────────────────────────────────────────┐
        │  adapters                                     │
        │  grpc/   http/   store/   （可互相不 import）   │
        └───────────────────┬─────────────────────────┘
                            │ 只调用
        ┌───────────────────▼─────────────────────────┐
        │  application/  用例编排（唯一业务入口）          │
        └───────────────────┬─────────────────────────┘
                            │ 只调用
        ┌───────────────────▼─────────────────────────┐
        │  domain/  实体 + 状态机 + 类型化错误（无 IO）    │
        └─────────────────────────────────────────────┘
```

## 各模块职责

### `server/src/domain/`（内层，无框架依赖）

- `error.rs` — 领域/应用层统一错误类型 `Error`：稳定 `code()`、`retryable()`、`safe_message()`（不外泄内部串）。
- `job.rs` — `JobStatus` 状态机（`queued → running → succeeded|failed|timed_out|cancelled`）与 `is_terminal()`。

### `server/src/application/`（用例编排，业务唯一入口）

- `auth_service.rs` — 登录、JWT 签发/校验、seed 默认管理员。
- `exec_service.rs` — 命令下发：建 Job → 经 `ConnectionRegistry` 推 `ExecRequest` → 置 running。
- `file_service.rs` — 文件上传/下载/列目录编排 + sha256 校验和（`CHUNK_SIZE = 64KiB`）。
- `forward_service.rs` — 正向连接：拨号 Agent、下发命令、收集输出与退出码。
- `scheduler.rs` — 定时任务：`schedule` 循环 + `resume_scheduled` 启动恢复。
- `listener_service.rs` — 监听器：create/list/start/stop + `resume_or_seed`（空表 seed 默认监听器，重启恢复 running 监听器）。
- `online_status.rs` — 心跳超时判定纯函数 `is_stale(last_seen, now, timeout)`。
- `agent_lifecycle_service.rs` — 注销（删 agent + 孤儿主机软删）+ 卸载下发。
- `cert_service.rs` — mTLS 证书底座：自签 CA + 签发 CSR；`load_or_generate`（`--tls-dir` 持久化 CA/server 证书，重启不换 CA）+ `issue_agent_cert` 离线签发 agent 三件套（forward 预置）。
- `audit_service.rs` — 审计记录落库 + 查询。
- `alert_service.rs` — 阈值告警判定（`threshold_for`）+ 落库。
- `process_service.rs` — 进程 list/kill + 网络信息（经 QueryRegistry 请求-应答）。
- `service_service.rs` — 常驻服务 CRUD + 启停/重启/日志。

HTTP 与 gRPC 适配器**都**调用本层，适配层之间禁止互相 import。

### `server/src/grpc/`（gRPC 适配层）

- `agent_service.rs` — `AgentServiceImpl`：处理反向 `OpenChannel` 双向流；首条须为 `Register`，token 严格匹配；落库后把入站流交给 `InboundCtx` 消费。纯函数 `token_matches` / `job_status` / `map_service_status` 也定义在此。
- `inbound.rs` — `InboundCtx`：reverse 与 forward **共用**的入站消息处理（心跳/指标/执行结果/文件/会话/服务/进程/网络），消除双份维护。
- `forward_manager.rs` — forward 持久连接管理器：reconciler 每 10s 对照 `hosts` 表（`conn_mode='forward' AND addr<>''`）差分启停拨号循环；拨号失败 5s 重连；mTLS 时走 https 双向认证；注册进 `ConnectionRegistry` 后**全端点对 forward 主机可用**。
- `connection_registry.rs` — 活跃连接注册表：`agent_id → mpsc::Sender<ServerMessage>`，单点路由。
- `transfer_registry.rs` — 文件传输等待表：upload 等 `FileStatus`、download 累积 chunk。
- `session_registry.rs` — 会话输出桥：`session_id → mpsc::Sender<Vec<u8>>`，把 Agent `SessionOutput` 转发给 WebSocket。
- `file_list_registry.rs` / `query_registry.rs` — 请求-应答等待表：`request_id → oneshot::Sender`（FileList / Process/NetInfo 查询）。
- `stream_registry.rs` — 实时流广播表：key（`service:{id}` / `job:{id}` / `metrics`）→ 订阅者列表，增量推送。
- `listener_registry.rs` — 动态监听器：`HashMap<Uuid, oneshot::Sender>` 关停句柄 + `start` 绑定 gRPC（可 mTLS）。

### `server/src/http/`（HTTP API 适配层，控制台）

- `mod.rs` — `AppState`、路由装配（`/healthz` 免认证、`/api/v1/*` 挂 JWT 中间件、WS 端点挂顶层）。
- `auth.rs` — 登录端点 + `require_auth` 中间件（claims 塞 request extension）。
- `error.rs` — 领域错误 → HTTP 响应的**单点错误边界**（内部细节只进日志）。
- 各端点模块：`hosts` / `agents` / `exec` / `jobs` / `metrics` / `files` / `tasks` / `forward` / `listeners` / `services` / `process` / `audit` / `alerts` / `cert` / `health` / `terminal`（WS）/ `stream`（WS 实时流）。

### `server/src/store/`（持久化适配层）

- `mod.rs` — `Db` 聚合根：连接池（max 10）+ `migrate()`（`sqlx::migrate!("./migrations")`）。
- 各 `*_repo.rs` — 按实体拆分仓储：host / agent / job / metric / file_transfer / task / user / listener / service / audit / alert。

### `agent/src/`（被控端）

- `connection.rs` — 反向模式：`run_agent` 外层重连循环（3s 间隔）+ `connect_once`（Register → 心跳 10s → 消费入站流）。
- `forward.rs` — 正向模式：`ForwardAgentServiceImpl` 监听，逻辑与反向同构。
- `exec.rs` — `run_and_report`：执行命令，回传 stdout/stderr 分块 + `finished` 结果。
- `file.rs` — `FileHandler`：upload 会话累积 chunk 写盘；download 读文件分块回传。
- `monitor.rs` — 每 30s 采集 `cpu.usage / mem.* / disk.usage / net.* / proc.count`。
- `pty.rs` — `SessionManager`：portable-pty 打开 PTY + shell，读输出回传 `SessionOutput`。
- `service.rs` — `ServiceManager`：常驻服务启动/停止 + 重启策略 + 日志增量上报。
- `process.rs` — 进程列表 / kill + 网络信息采集（sysinfo `Networks`）。
- `fs.rs` — `list_dir`：目录浏览（排序 + 权限）。
- `cert.rs` — mTLS：反向模式生成 CSR → HTTP 换证书 → 本地缓存；forward 模式 `load_cached` 加载管理员预置的三件套（缺任一文件拒绝启动）。
- `encoding.rs` — 控制台输出解码：Windows 按 OEM 代码页（`GetOEMCP()`，如中文 936/GBK）解码为 UTF-8，其他平台 UTF-8 lossy。
- `uninstall.rs` — `self_destruct`：移除自启 + 删二进制（Windows 延迟删除）+ 退出。

## 运行时流程

反向模式（默认）关键链路：

```text
Agent 启动 → 拨号 Server gRPC → OpenChannel 双向流 → 首条 Register(token, host_info)
  → Server 校验 token → 落库 host+agent（事务）→ 注册到 connection_registry → 回 RegisterAck
  → Agent 心跳循环(10s) + 监控循环(30s)
控制台 → HTTP POST /api/v1/exec → ExecService 建 Job(queued) → registry.send(ExecRequest)
  → Agent 执行 → 流式 ExecResult(stdout/stderr 分块 + finished) → Server 落库 Job 终态
```

交互终端链路（Phase 6）：

```text
控制台 WS GET /agents/{id}/terminal?token= → AuthService.verify → SessionOpen 下发 Agent
  → Agent PTY 启动 shell → SessionOutput 回传 → SessionRegistry.forward → WS 二进制帧
```

forward 持久连接链路（2026-09）：

```text
Server 启动 → ForwardManager.spawn_reconciler（每 10s 与 hosts 表差分）
  → 对 conn_mode='forward' 且 addr<>'' 的 host 拨号（mTLS 时 https 双向认证）
  → Agent(ForwardAgentService) 先发 Register → token 校验 → AgentRepo::register_under_host 挂到既定 host
  → 注册进 ConnectionRegistry → InboundCtx 消费入站流（与 reverse 同构）
  → 此后全部控制端点（terminal / files / services / processes / net / exec）按 agent_id 路由，HTTP 层零改动
```

实时流链路（Phase 8）：

```text
Agent 增量（ExecResult chunk / ServiceStatus log / MetricReport）
  → Server StreamRegistry.broadcast(key) → 订阅该 key 的 WS 端点推给前端
```

详见 [docs/plantree/baseline/runtime-flows.md](plantree/baseline/runtime-flows.md)。

## 规划树（plantree）边界

`docs/plantree/` 是项目的规划与决策树（baseline / plans / decisions / topics）。
本归档（`docs/*.md`）以**当前实现**为准；plantree 中的**决策链**（001–008）与 roadmap
（Phase 0–8）保留历史规划性质，落地状态见 [roadmap.md](plantree/plans/server/roadmap.md)。
