# 未决问题

Role: open-questions
Status: active

仅列未解决的问题。已解决/已决策的问题移入 `decisions/`。

1. **前端控制台如何访问 Server？** REST（axum）vs gRPC-web vs 两者。影响 Server 的 HTTP 层设计。
   - 倾向：REST + JSON 给控制台，gRPC 专用于 Agent 信道（浏览器无法直连原生 gRPC）。

2. ~~**proto 契约的物理位置？**~~ ✅ 已解决：workspace 顶层独立 `proto` crate（`helm-proto`），Agent 与 Server 都依赖它。

3. ~~**存储是否直接上 Postgres？**~~ ✅ 已解决：直接 Postgres（决策 004）。

4. **正向连接模式的触发与调度。** Server 何时主动连 Agent（按需 vs 常驻连接池 vs 调度器）？

5. **RBAC 与多租户是否 MVP 就要。** 目前 Deferred，但会影响数据模型（是否预留 org/tenant 字段）。

6. **认证强度的起步水位。** 当前仅 token（明文 gRPC，TLS 未做），何时上 TLS/mTLS、token 轮换机制。
