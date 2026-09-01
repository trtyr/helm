# 005 — 监听器作为一等公民（DB 实体 + API 管理）

Date: 2026-09-01

## Context

当前 Server 的 gRPC 监听在 `server/src/grpc/mod.rs` 硬编码 `config.grpc_addr`，
进程启动即监听，无法经 API 管理。C2 运维平台需要可配置、可多开、可启停的
「监听器」（Agent 的接入点），这是 Phase 5 的核心。

## Decision

- 引入「监听器」实体：`id` / `name` / `addr` / `proto`（先 gRPC）/ `auth`（token）/ `status`（running / stopped）。
- 监听器持久化到 Postgres（新增 `listeners` 表），Server 启动时从 DB 恢复已启用的监听器。
- 经 HTTP API 管理：`POST /api/v1/listeners`（创建）、`GET /api/v1/listeners`（列表）、`POST /api/v1/listeners/{id}/start|stop`（启停）。

## Consequences

### 启用

- 接入点可管理、可多开、可热启停，接近 C2 的「监听器」语义。
- 配置随库持久化，重启后自动恢复。

### 约束/代价

- 需监听器生命周期管理（端口冲突、优雅启停）。
- 现有 `config.grpc_addr` 需迁移为「默认监听器」或保留为兜底。

**相关：** [006 在线判定](006-online-status-detection.md)、
[topics/listener-management](../topics/listener-management.md)
