# Plan Tree — Helm 运维平台

Role: entrypoint
Status: active

集中式运维平台的规划与状态根。记录项目方向、架构底座、计划进度、决策链与落地证据，跨会话、跨 Agent 可恢复。

## 权威顺序（Authority Order）

冲突时以下面的顺序为准（前者优先）：

1. 本 README 的已注册 plan 与 baseline 链接
2. `baseline/` 中的项目级上下文（模块地图、运行时流程、存储、门禁、风险）
3. 各 plan root 的 `roadmap.md` 与 `decisions/`（具体计划）
4. `implementation-status.md`（仅 `In Progress` 计划的操作性交接）

## 当前状态

| Plan | 状态 | 阶段 | 最后落地 | 下一步 |
|------|------|------|----------|--------|
| [server](plans/server/README.md) | Planning | 底座设计 | — | 确认底座设计 → 建 workspace + proto 契约 |

## 如何阅读

1. 先读 [baseline](baseline/README.md)，了解项目级上下文与目标架构。
2. 再读目标 plan root 的 `README.md` 与 `roadmap.md`。
3. 需要细节时钻入 `topics/`（胶囊）与 `decisions/`（决策链）。
4. 新想法先丢 [ideas/inbox.md](ideas/inbox.md)，成熟后 promote。

## 边界

本树只治理**后端**（Server 控制端 + 共享 protobuf 契约）。Agent 被控端将另立 plan root，但其共享契约在 `plans/server/` 中定义。前端控制台独立工程，不在此树范围内（仅定义其访问后端的接口）。
