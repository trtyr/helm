# 目标模块地图

Role: topic-capsule（baseline 全局上下文）
Status: planning

## 系统两大组件

```text
┌────────────────────┐         ┌────────────────────┐
│   Server 控制端     │         │   Agent 被控端      │
│  （本 plan 焦点）    │◄───────►│  （另立 plan）      │
│  axum + gRPC        │  gRPC   │  tokio, 跨平台      │
└────────────────────┘         └────────────────────┘
         ▲
         │ HTTP API（前端控制台）
         ▼
   独立前端工程（不在本树）
```

- **Server**：集中管理、任务编排、状态收集、持久化、对外 API。
- **Agent**：装在目标机（Windows/Linux），执行命令、采集状态、文件传输。
- **共享 proto**：两者之间的契约，是底座，先于一切代码定义。

## Server 分层（六边形 / clean architecture）

依赖方向：外层适配器 → 内层领域。领域不依赖任何 IO 框架。

```text
server/
├── proto/                      # protobuf 契约（共享底座）
│   └── agent/v1/
│       ├── agent.proto         # AgentService：注册/心跳/状态/命令流
│       └── types.proto         # 共享类型
├── src/
│   ├── main.rs                 # 入口：装配、配置、优雅退出
│   ├── config/                 # 配置加载与校验
│   ├── domain/                 # 纯领域模型（无 IO）
│   │   ├── host.rs             # 主机
│   │   ├── agent.rs            # Agent 实例/连接状态
│   │   ├── task.rs             # 任务定义
│   │   ├── job.rs              # 执行实例 + 状态机
│   │   ├── file_transfer.rs    # 文件传输
│   │   └── metric.rs           # 状态指标
│   ├── application/            # 用例编排层（业务规则）
│   │   ├── host_service.rs
│   │   ├── task_service.rs
│   │   └── agent_conn.rs       # 活跃连接路由
│   ├── grpc/                   # gRPC 适配层（Agent 反向连入）
│   │   ├── agent_service.rs    # AgentService 实现
│   │   └── connection_registry.rs
│   ├── http/                   # HTTP API 适配层（前端控制台）
│   │   ├── routes/             # hosts/tasks/jobs/files/metrics
│   │   └── middleware/         # 认证、日志、错误边界
│   ├── store/                  # 持久化适配层（sqlx）
│   │   ├── migrations/
│   │   └── *_repo.rs           # 各实体仓储
│   └── telemetry/              # tracing + metrics + health
```

## 分层边界（AI 可导航性）

- `domain` 是深模块：实体 + 状态机 + 不变量，无外部依赖。
- `application` 是唯一编排入口：HTTP 与 gRPC 适配器都调它，不互相调用。
- 适配层（grpc/http/store）之间**禁止**互相 import。
- 错误处理：`domain` 定义类型化错误（`code` + `retryable`），适配层做单点日志边界。

## Agent 模块（另立 plan，仅概览）

- 单二进制，Windows/Linux 各自编译，tokio 异步。
- 反向模式：gRPC client 主动连 Server；正向模式：gRPC server 监听等 Server 连。
- 能力模块：exec / file / sysinfo / task。
- 与 Server 共享 `proto/`，不复制契约。

详细契约与实现：见 [plans/server/topics/](../plans/server/topics/)。
