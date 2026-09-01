# Roadmap — Server

Role: current-state
Status: active

阶段划分遵循「底座先行」：Phase 0/1 是底座，Phase 2+ 是功能。底座未稳不进入功能期。

## Done（已落地，有可复现证据）

- **Phase 0 — 工程骨架与底座** ✅
  - Cargo workspace：`server` + `agent` + `proto` 共享 crate
  - protobuf 契约：`agent/v1/agent.proto`、`types.proto`（双向流服务定义）
  - 分层目录 + 模块边界（domain/application/grpc/http/store）
  - 配置加载（clap）+ 可观测性（tracing + `/healthz`）
  - 存储底座：sqlx + Postgres migrations + 实体 schema
  - 门禁：fmt + clippy + test + `buf` 契约检查
- **Phase 1 — 通信底座 + 最小链路** ✅
  - gRPC server 双向流 + connection_registry
  - Agent 反向连接：注册 + token 认证 + 心跳
  - 命令执行链路：下发命令 → 流式拿回输出
  - 断线重连
- **Phase 2 — HTTP API + 存储落地** ✅
  - axum HTTP API：hosts / tasks / jobs / files / metrics
  - sqlx 仓储落地，Job 状态机落库
  - 控制台认证（JWT）
- **Phase 3 — 功能补全** ✅
  - 文件传输（分块 + 进度 + 校验和）
  - 状态监控（指标采集 + 落库）
  - 脚本/任务下发（定时调度 + 重启恢复）

## In Progress

- **Phase 4 — 正向连接 + 强化**（部分完成）
  - ✅ Agent 正向监听模式（Server 主动连）
  - ⬜ mTLS、RBAC 强化、审计日志（未做，见 risk-hotspots 与 ideas/inbox）

## Deferred

- 多租户
- Web 终端（实时交互 shell）
- Agent 插件机制
- 高可用/集群部署

## 落地证据约定

每个 Phase 的「完成」须有可复现证据（见 [baseline 门禁](../../baseline/test-and-release-gates.md)）：
构建通过、测试通过、契约检查通过、以及该 Phase 的链路 smoke 结果。
当前落地证据：`git log`（`2b97ae8` 底座 + 通信链路，`c0456f0` 正向连接 API），
以及 [docs/current-state.md](../../../current-state.md) 的实测门禁结果（fmt/clippy/test/buf 全绿）。
