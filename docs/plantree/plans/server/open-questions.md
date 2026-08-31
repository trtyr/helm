# 未决问题

Role: open-questions
Status: active

仅列未解决的问题。已解决/已决策的问题移入 `decisions/`。

1. **前端控制台如何访问 Server？** REST（axum）vs gRPC-web vs 两者。影响 Server 的 HTTP 层设计。
   - 倾向：REST + JSON 给控制台，gRPC 专用于 Agent 信道（浏览器无法直连原生 gRPC）。

2. **proto 契约的物理位置？** 放 `server/` 内共享，还是独立 crate / 独立 git repo？
   - 倾向：先放 workspace 内独立 `proto` crate，Agent 与 Server 都依赖它；多团队/多语言时再抽独立 repo。

3. **存储是否直接上 Postgres？** 用户强调底座先行，SQLite 起步是否为「妥协」。
   - 倾向：sqlx 抽象 + SQLite 起步（零部署跑通），迁移机制建好，切 Postgres 成本可控。

4. **正向连接模式的触发与调度。** Server 何时主动连 Agent（按需 vs 常驻连接池 vs 调度器）？

5. **RBAC 与多租户是否 MVP 就要。** 目前 Deferred，但会影响数据模型（是否预留 org/tenant 字段）。

6. **认证强度的起步水位。** 起步 TLS + token，何时上 mTLS、token 轮换机制。
