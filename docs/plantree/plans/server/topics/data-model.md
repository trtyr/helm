# 数据模型

Role: topic-capsule
Status: planning
Read when: 需要了解实体、状态机、存储 schema
Related: [decisions/003](../decisions/003-storage-sqlx-sqlite-first.md)

## One-Screen Summary

核心实体：Host / Agent / Task / Job / FileTransfer / Metric / User。
Job 与 FileTransfer 有显式状态机。存储走 sqlx + migrations。

## Current Position

实体模型与状态机已定义，schema 未落地。

## Active Constraints

- schema 含外键、索引、时间戳、软删除标记，从第一天起生产标准。
- 不用 SQLite 专有特性，保证可迁 Postgres。
- Job/FileTransfer 有稳定 ID，支持幂等重放与去重。

## Open Risks Or Questions

- 是否预留 tenant/org 字段以兼容未来多租户 → open-questions#5
- 大规模 Metric 是否迁独立时序库（Deferred）

## Details

- 完整实体表见 [baseline storage-and-state](../../../baseline/storage-and-state.md)
