# 连接模型

Role: topic-capsule
Status: active
Read when: 需要理解正/反向连接、双向流、断线重连、会话状态
Related: [decisions/002](../decisions/002-bidirectional-stream-connection-model.md)

## One-Screen Summary

一条 gRPC 双向流统一两种连接模式。反向（Agent 主动连）为默认，
正向（Server 主动连）同构复用同一信令协议。Agent 在线 = 存在活跃流。

## Current Position

已落地：connection_registry 是 Server 侧活跃连接事实来源；反向重连已实现（3s 退避）。

## Active Constraints

- 反向：Agent 是 client，Server 是 listener（`AgentService.OpenChannel`）。
- 正向：Agent 是 server，Server 是 client，拨号后建同一条双向流。
- 重连后重新 Register（已实现）；未完成 Job 结果重放与消息幂等**未实现**（见 runtime-flows 关键不变量）。

## Open Risks Or Questions

- 心跳超时阈值、重连退避策略的取值（未定）
- 正向模式的触发/调度方式 → open-questions#4
- 半开连接检测（HTTP/2 keepalive / PING）机制待落地

## Details

- 运行时流程见 [baseline runtime-flows](../../../baseline/runtime-flows.md)
- 断线重连是风险热点，见 [baseline risk-hotspots](../../../baseline/risk-hotspots.md)
