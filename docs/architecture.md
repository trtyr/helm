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
│   ├── migrations/               # sqlx 迁移（3 个版本）
│   └── tests/                    # 集成测试（连真实 Postgres）
├── agent/                # Agent 被控端（单二进制、跨平台）
│   └── src/
│       ├── main.rs               # 入口：按 conn_mode 分派
│       ├── config/               # clap 配置
│       ├── connection.rs         # 反向：拨号 Server + 双向流 + 心跳 + 重连
│       ├── forward.rs            # 正向：gRPC server 监听
│       ├── exec.rs               # 命令执行并回报
│       ├── file.rs               # 文件传输（upload 会话 + download）
│       ├── monitor.rs            # 指标采集（sysinfo）
│       └── telemetry/            # tracing 初始化
├── scripts/              # e2e smoke / 调度恢复 e2e
├── docs/plantree/        # 规划树（见 README 边界说明）
└── src/main.rs           # ⚠ 孤儿 hello world，不在 workspace 内（见 current-state）
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
- `file_service.rs` — 文件上传/下载编排 + sha256 校验和（`CHUNK_SIZE = 64KiB`）。
- `forward_service.rs` — 正向连接：拨号 Agent、下发命令、收集输出与退出码。
- `scheduler.rs` — 定时任务：`schedule` 循环 + `resume_scheduled` 启动恢复。

HTTP 与 gRPC 适配器**都**调用本层，适配层之间禁止互相 import。

### `server/src/grpc/`（gRPC 适配层，Agent 反向连入）
- `agent_service.rs` — `AgentServiceImpl`：处理 `OpenChannel` 双向流；首条须为 `Register`，token 严格匹配；后台 task 消费入站流（心跳/指标/执行结果/文件 chunk），流结束注销。
- `connection_registry.rs` — 活跃连接注册表：`agent_id → mpsc::Sender<ServerMessage>`，单点路由。
- `transfer_registry.rs` — 文件传输等待表：upload 等 `FileStatus`、download 累积 chunk。

### `server/src/http/`（HTTP API 适配层，控制台）
- `mod.rs` — `AppState`、路由装配（`/healthz` 免认证、`/api/v1/*` 挂 JWT 中间件）。
- `auth.rs` — 登录端点 + `require_auth` 中间件（claims 塞 request extension）。
- `error.rs` — 领域错误 → HTTP 响应的**单点错误边界**（内部细节只进日志）。
- 其余各端点：`hosts` / `exec` / `jobs` / `metrics` / `files` / `tasks` / `forward` / `health`。

### `server/src/store/`（持久化适配层）
- `mod.rs` — `Db` 聚合根：连接池（max 10）+ `migrate()`（`sqlx::migrate!("./migrations")`）。
- 各 `*_repo.rs` — 按实体拆分仓储（host / agent / job / metric / file_transfer / task / user）。

### `agent/src/`（被控端）
- `connection.rs` — 反向模式：`run_agent` 外层重连循环（3s 间隔）+ `connect_once`（Register → 心跳 10s → 消费入站流）。
- `forward.rs` — 正向模式：`ForwardAgentServiceImpl` 监听，逻辑与反向同构。
- `exec.rs` — `run_and_report`：执行命令，回传 stdout/stderr 分块 + `finished` 结果。
- `file.rs` — `FileHandler`：upload 会话累积 chunk 写盘；download 读文件分块回传。
- `monitor.rs` — 每 30s 采集 `cpu.usage / mem.* / proc.count`。

## 运行时流程

反向模式（默认）关键链路：

```text
Agent 启动 → 拨号 Server gRPC → OpenChannel 双向流 → 首条 Register(token, host_info)
  → Server 校验 token → 落库 host+agent（事务）→ 注册到 connection_registry → 回 RegisterAck
  → Agent 心跳循环(10s) + 监控循环(30s)
控制台 → HTTP POST /api/v1/exec → ExecService 建 Job(queued) → registry.send(ExecRequest)
  → Agent 执行 → 流式 ExecResult(stdout/stderr 分块 + finished) → Server 落库 Job 终态
```

详见 [docs/plantree/baseline/runtime-flows.md](plantree/baseline/runtime-flows.md)（规划稿，流程与实现一致）。

## 规划树（plantree）边界

`docs/plantree/` 是项目早期的规划与决策树（baseline / plans / decisions）。
**注意**：其中部分内容已过时——`baseline/README.md` 仍称项目为"空壳 hello world"，
`baseline/module-map.md` 是目标设计而非现状。本归档（`docs/*.md`）以**当前实现**为准，
plantree 中的**决策链**（001–004）仍有效，可交叉参考。
