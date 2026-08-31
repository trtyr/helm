# 001 — 通信底座用 gRPC

Date: 2026-08-31

## Context

Server 与 Agent 之间需要长连接、双向、强类型的 RPC 信道。
最初的方案提过「WebSocket + JSON 最快跑通」，但用户明确要求底座先行，
拒绝为 MVP 放弃架构。

## Decision

通信底座直接采用 **gRPC（HTTP/2 + protobuf）**，不上 WebSocket + JSON。

## Consequences

### 启用

- 强类型契约（protobuf）：Server/Agent 接口以 `.proto` 为单一事实来源，编译期保证一致。
- 双向流（bidirectional stream）：一条连接承载注册/心跳/命令/状态/文件全部信令。
- 自带认证、超时、重试、流控、健康检查语义（HTTP/2 层）。
- 生态：`tonic`（Rust）+ `buf` 契约检查 + grpcurl 调试。
- 未来跨语言客户端（前端 gRPC-web、其他语言 Agent）可复用契约。

### 约束/代价

- 浏览器无法直连原生 gRPC → 控制台走 REST/JSON 或 gRPC-web（见 open-questions#1）。
- 比 WebSocket+JSON 稍重：需要 proto 编译、tonic 依赖、学习曲线。
- protobuf 演进须守向后兼容规则（`buf breaking` 门禁）。

**相关：** [communication-protocol](../topics/communication-protocol.md)、
[connection-model](../topics/connection-model.md)、
[契约门禁](../../../baseline/test-and-release-gates.md)
