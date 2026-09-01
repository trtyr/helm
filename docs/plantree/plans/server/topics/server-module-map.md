# Server 模块地图

Role: topic-capsule
Status: active
Read when: 需要了解 Server 分层、模块边界、依赖方向
Related: [baseline module-map](../../../baseline/module-map.md)

## One-Screen Summary

Server 用六边形/clean architecture 分层：
`domain`（纯逻辑）← `application`（编排）← 适配层（grpc/http/store）。
依赖只朝内，适配层之间禁止互调。

## Current Position

目录已建（`server/src/{domain,application,grpc,http,store}`），分层边界与依赖方向已落地。

## Active Constraints

- `domain` 深模块：实体 + 状态机 + 不变量，无 IO 依赖。
- `application` 是唯一编排入口，HTTP 与 gRPC 都调它。
- 错误处理：domain 定义类型化错误（code + retryable），适配层单点日志边界。
- store 为各实体仓储（`*_repo.rs`，sqlx 直查，未做 trait 抽象）。

## Open Risks Or Questions

- proto 归入 workspace 的哪个位置 → 已解决：顶层独立 `proto` crate（open-questions#2）
- 是否引入 CQRS / event sourcing（当前倾向不引入，保持简单）

## Details

- 完整分层见 [baseline module-map](../../../baseline/module-map.md)
