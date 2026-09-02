# 模块地图

Role: topic-capsule（baseline 全局上下文）
Status: active

## 系统两大组件

```text
┌────────────────────┐         ┌────────────────────┐
│   Server 控制端     │         │   Agent 被控端      │
│  axum + tonic      │◄───────►│  tokio, 跨平台      │
│  + sqlx/Postgres   │  gRPC   │  exec/file/monitor  │
└────────────────────┘         └────────────────────┘
         ▲
         │ HTTP API（控制台）
         ▼
   独立前端工程（不在本树）
```

- **Server**：集中管理、任务编排、状态收集、持久化（Postgres）、对外 HTTP API。
- **Agent**：装在目标机（Windows / Linux / macOS），执行命令、采集指标、文件传输。
- **共享 proto**：两者之间的契约，是底座，先于一切代码定义。

## Server 分层（六边形 / clean architecture）

依赖方向：外层适配器 → 内层领域。领域不依赖任何 IO 框架。

```text
server/
├── src/
│   ├── main.rs / lib.rs        # 入口 + run() 装配
│   ├── config/                 # clap 配置（env 覆盖）
│   ├── domain/                 # 纯领域：error.rs（类型化错误）+ job.rs（状态机）
│   ├── application/            # 用例编排：auth/exec/file/forward/scheduler/listener/online_status/agent_lifecycle/cert/audit/alert/process/service
│   ├── grpc/                   # gRPC 适配：agent_service + connection/transfer/session/file_list/query/stream/listener registry
│   ├── http/                   # HTTP API 适配：auth/exec/files/hosts/agents/jobs/metrics/tasks/forward/listeners/services/process/audit/alerts/cert/terminal/stream/health
│   ├── store/                  # 持久化：Db 聚合根 + 各 *_repo.rs
│   └── telemetry/              # tracing 初始化
├── migrations/                 # sqlx 迁移（0001..0007）
└── tests/                      # 集成测试（连真实 Postgres）
proto/                          # 共享 protobuf 契约（workspace 顶层独立 crate）
```

## 分层边界（AI 可导航性）

- `domain` 是深模块：实体 + 状态机 + 不变量，无外部依赖（目前只有 error + job）。
- `application` 是唯一编排入口：HTTP 与 gRPC 适配器都调它，不互相调用。
- 适配层（grpc/http/store）之间**禁止**互相 import。
- 错误处理：`domain` 定义类型化错误（`code` + `retryable` + `safe_message`），适配层做单点日志边界。

## Agent 模块

- 单二进制，Windows / Linux / macOS 各自编译，tokio 异步。
- 反向模式：gRPC client 主动连 Server（`connection.rs`）；正向模式：gRPC server 监听（`forward.rs`）。
- 能力模块：exec（命令执行）/ file（文件传输）/ monitor（指标采集）/ pty（交互终端）/ service（常驻服务）/ fs（目录浏览）/ process（进程/网络）/ cert（mTLS 证书）/ uninstall（自杀卸载）。

详细实现见 [docs/architecture.md](../../architecture.md)。
