# 数据模型

Role: topic-capsule
Status: active
Read when: 需要了解实体、状态机、存储 schema
Related: [decisions/004](../decisions/004-storage-postgres-direct.md)

## One-Screen Summary

核心实体：Host / Agent / Task / Job / FileTransfer / Metric / User。
Job 与 FileTransfer 有显式状态机。存储走 sqlx + migrations。

## Current Position

schema 已落地（Postgres + 3 个迁移，见 `server/migrations/`）。

## Active Constraints

- 直接 Postgres（决策 004）；schema 含外键、索引、时间戳、软删除，从第一天起生产标准。
- Job/FileTransfer 有稳定 ID；幂等重放与去重尚未实现。

## Open Risks Or Questions

- 是否预留 tenant/org 字段以兼容未来多租户 → open-questions#5
- 大规模 Metric 是否迁独立时序库（Deferred）

## Details

- 完整实体表见 [baseline storage-and-state](../../../baseline/storage-and-state.md)
