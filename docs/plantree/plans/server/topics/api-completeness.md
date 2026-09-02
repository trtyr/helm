# API 完整性

Role: topic-capsule
Status: active
Read when: 需要了解 HTTP API 全量化（CRUD / 实时流 / 分页 / 详情）的规划
Related: [roadmap](../roadmap.md)

## One-Screen Summary

把 HTTP API 从「基础读写」补到「完整操作面」：DELETE/UPDATE、分页/过滤、WebSocket 实时流、
agents 独立列表/详情、批量操作。对应 Phase 8。

## Current Position

已实现 12 个端点：`auth/login`、`hosts`（GET/POST）、`exec`、`jobs/{id}`、`metrics`、
`files/upload|download`、`tasks/script|schedule`、`forward/exec`、`healthz`。
缺失：DELETE/UPDATE、在线状态、实时流、分页、agents 详情、批量。规划中，未实现。

## Active Constraints

- 控制台走 REST + JSON，实时输出走 WebSocket（见 open-questions#1 倾向）。
- 统一错误格式沿用现有 `{error: {code, message}}` 单点边界。
- agents 与 hosts 分离：agents 暴露连接状态/版本，hosts 暴露主机资产。

## Open Risks Or Questions

- 前端控制台接入方式（REST vs gRPC-web）→ open-questions#1。

## Details

- 完成标准见 [roadmap Phase 8](../roadmap.md)。
