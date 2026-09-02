# 监听器管理

Role: topic-capsule
Status: active
Read when: 需要了解「监听器」——Agent 接入点的模型与管理
Related: [decisions/005](../decisions/005-listener-model.md)

## One-Screen Summary

监听器是 Agent 的接入点，已提升为一等公民：DB 实体 + API 启停 + 重启恢复，接近 C2 的「监听器」语义。

## Current Position

已实现（Phase 5）：监听器为 `listeners` 表实体（id/name/addr/proto/auth/status），支持
create/list/start/stop + 多实例 + 重启恢复；默认监听器由 `resume_or_seed` seed/resume。

## Active Constraints

- 先只做 gRPC 监听器，多协议（HTTP/WebSocket）为开放问题。
- 监听器实体：`id` / `name` / `addr` / `proto` / `auth` / `status`，持久化到新增 `listeners` 表。
- 生命周期：端口冲突检测、优雅启停、Server 启动时从 DB 恢复 running 状态监听器。
- 迁移：现有 `config.grpc_addr` 需改为「默认监听器」或保留兜底。

## Open Risks Or Questions

- 监听器是否需要多协议 → open-questions#8。
- 端口冲突 / 热重启时的在途连接如何处理。

## Details

- 管理 API：`POST /api/v1/listeners`、`GET /api/v1/listeners`、`POST /api/v1/listeners/{id}/start|stop`。
- 完整决策见 [decisions/005](../decisions/005-listener-model.md)。
