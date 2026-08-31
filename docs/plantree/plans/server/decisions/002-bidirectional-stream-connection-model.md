# 002 — 双向流统一正/反向连接模型

Date: 2026-08-31

## Context

用户要求正向（同区域内网：Server 主动连 Agent）与反向
（跨公网：Agent 主动连 Server）两种连接模式都要支持。
传统做法会给两种模式各写一套通信逻辑，维护两份。

## Decision

用**一条 gRPC bidirectional stream** 统一两种模式：

- 反向模式（默认）：Agent 作为 gRPC client 拨号 Server，
  调 `AgentService.OpenChannel` 建立双向流，Server 端是 listener。
- 正向模式：Agent 作为 gRPC server 监听，Server 作为 client 拨号，
  建立同一条双向流。

两种模式的**信令协议完全同构**，区别仅在「谁发起 TCP 连接、谁监听」。

## Consequences

### 启用

- 一套信令协议、一套处理逻辑，正/反向只是连接建立方式的差异。
- Agent 在线 = 存在活跃流，状态推导单一可靠。
- 反向模式天然穿透 NAT/防火墙（Agent 出站），开箱即用。

### 约束/代价

- 两端都要同时实现 gRPC client 与 server 角色（Agent 尤甚）。
- 连接生命周期、断线重连、幂等重放需要精心设计
  （见 [connection-model](../topics/connection-model.md)、
  [runtime-flows](../../../baseline/runtime-flows.md)）。

**相关：** [connection-model](../topics/connection-model.md)、
[001 gRPC 底座](001-grpc-as-communication-base.md)
