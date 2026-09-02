# Plan Tree — Helm 运维平台

Role: entrypoint
Status: active

集中式运维平台的规划与状态根。记录项目方向、架构底座、计划进度、决策链与落地证据，跨会话、跨 Agent 可恢复。

## 权威顺序（Authority Order）

冲突时以下面的顺序为准（前者优先）：

1. 本 README 的已注册 plan 与 baseline 链接
2. `baseline/` 中的项目级上下文（模块地图、运行时流程、存储、门禁、风险）
3. 各 plan root 的 `roadmap.md` 与 `decisions/`（具体计划）
4. 实测落地证据见 [docs/current-state.md](../current-state.md)

## 当前状态

| Plan | 状态 | 阶段 | 最后落地 | 下一步 |
|------|------|------|----------|--------|
| [server](plans/server/README.md) | Active | Phase 0–8 已落地 | Phase 8 API 完整性（`a3e0300`） | — |

## 如何阅读

1. 先读 [baseline](baseline/README.md)，了解项目级上下文与目标架构。
2. 再读目标 plan root 的 `README.md` 与 `roadmap.md`。
3. 需要细节时钻入 `topics/`（胶囊）与 `decisions/`（决策链）。
4. 新想法先丢 [ideas/inbox.md](ideas/inbox.md)，成熟后 promote。

## 边界

本树治理**后端**（Server 控制端 + Agent 被控端 + 共享 proto），以运维平台能力为主线（参考 C2 控制面：监听器、在线状态、会话、服务管理、文件、控制、持久化、安全、API；不做隐蔽性设计）。Agent 内部的深度设计（如插件机制）另立 plan root；前端控制台独立工程，不在此树范围内（仅定义其访问后端的接口）。
