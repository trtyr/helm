# Roadmap — Server

Role: current-state
Status: planning

阶段划分遵循「底座先行」：Phase 0/1 是底座，Phase 2+ 是功能。底座未稳不进入功能期。

## In Progress

- **Phase 0 — 工程骨架与底座**（当前聚焦）
  - Cargo workspace：`server` + `agent` + `proto` 共享 crate（或独立 proto 包）
  - protobuf 契约：`agent/v1/agent.proto`、`types.proto`（双向流服务定义）
  - 分层目录 + 模块边界（domain/application/grpc/http/store）
  - 配置加载与校验（figment/clap）
  - 可观测性底座：tracing + 结构化日志 + health 端点
  - 存储底座：sqlx + migrations + 实体 schema
  - 门禁：fmt + clippy + test + `buf` 契约检查

## Next

- **Phase 1 — 通信底座 + 最小链路**
  - gRPC server 起服务，双向流 + connection_registry
  - Agent 反向连接：注册 + token 认证 + 心跳
  - 命令执行链路：下发命令 → 流式拿回输出（先跑通 echo）
  - 断线重连 + 心跳超时检测
- **Phase 2 — HTTP API + 存储落地**
  - axum HTTP API：hosts / tasks / jobs / files / metrics
  - sqlx 仓储落地，Job 状态机落库
  - 控制台认证（JWT/session）
- **Phase 3 — 功能补全**
  - 文件传输（分块 + 进度 + 校验和）
  - 状态监控（指标采集 + 看板数据）
  - 脚本/任务下发（定时调度）
- **Phase 4 — 正向连接 + 强化**
  - Agent 正向监听模式（Server 主动连）
  - mTLS、RBAC 强化、审计日志

## Deferred

- 多租户
- Web 终端（实时交互 shell）
- Agent 插件机制
- 高可用/集群部署

## 落地证据约定

每个 Phase 的「完成」须有可复现证据（见 [baseline 门禁](../../baseline/test-and-release-gates.md)）：
构建通过、测试通过、契约检查通过、以及该 Phase 的链路 smoke 结果。
