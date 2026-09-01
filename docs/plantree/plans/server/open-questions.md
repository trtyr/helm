# 未决问题

Role: open-questions
Status: active

仅列未解决的问题。已解决/已决策的问题移入 `decisions/` 或标注去向。

1. **前端控制台如何访问 Server？** REST（axum）vs gRPC-web vs 两者。影响 Server 的 HTTP 层设计。
   - 倾向：REST + JSON 给控制台，gRPC 专用于 Agent 信道；实时输出走 WebSocket（见 Phase 8）。

2. ~~**proto 契约的物理位置？**~~ ✅ 已解决：workspace 顶层独立 `proto` crate（`helm-proto`）。

3. ~~**存储是否直接上 Postgres？**~~ ✅ 已解决：直接 Postgres（决策 004）。

4. **正向连接是否需要连接池/调度？** 按需拨号（`POST /api/v1/forward/exec`）已落地；是否需常驻连接池/调度器待定（Phase 5+ 评估）。

5. **多租户是否预留？** 权限已定：单用户，暂不 RBAC（007 已改）；多租户仍 Deferred，是否预留 org/tenant 字段待定。

6. **token 轮换与吊销机制。** mTLS 已决策（007，Phase 7）；token 轮换/吊销时机待定。

7. **交互 shell 的实现方式。** WebSocket 流式 + PTY（跨平台伪终端）细节未定，Windows PTY 是难点。→ Phase 6

8. **监听器是否需多协议。** 先 gRPC（005）；是否需要 HTTP/WebSocket 监听器待定。→ Phase 5

9. **mTLS 证书分发/轮换机制。** 自签 CA vs 现有 PKI；证书如何下发到 Agent、如何轮换。→ Phase 7

10. **大规模 agent 的在线状态聚合性能。** 内存注册表（006）+ last_seen 查询在 agent 数量大时是否需要 DB 物化/缓存。→ Phase 5
