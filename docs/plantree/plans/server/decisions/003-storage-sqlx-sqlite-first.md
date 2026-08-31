# 003 — 存储：sqlx + SQLite 起步，Postgres 目标

> **Superseded** — 由 [004](004-storage-postgres-direct.md) 取代（2026-08-31）：
> 用户选择直接上 Postgres，不再用 SQLite。

Date: 2026-08-31

## Context

底座要建好，但项目从零起步，需要尽快跑通核心链路。
「是否直接上 Postgres」是 open-questions#3，此处给出倾向性决策。

## Decision

- 持久层用 **sqlx**（异步、编译期 SQL 检查、多数据库抽象）。
- 起步数据库用 **SQLite**（零部署、单文件、方便本地跑通与测试）。
- 目标数据库为 **PostgreSQL**，切换仅改连接串 + 迁移方言。

## Consequences

### 启用

- schema、迁移机制（`sqlx migrate`）、索引、外键从第一天就按生产标准建。
- 不在 SQLite 上使用任何专有特性，保证可平滑迁 Postgres。
- 本地开发/CI 零外部依赖，MVP 链路验证成本极低。

### 约束/代价

- SQLite 并发写能力弱，生产多写场景必须切 Postgres。
- 时间序列指标（Metric）在大规模时需评估是否单独存时序库，
  此为 Deferred（见 risk-hotspots「存储迁移」）。

**注意：** 若后续用户明确要求「一开始就 Postgres」，此决策可被 004 决策取代，
但 sqlx + 迁移机制不变，只是替换驱动。

**相关：** [data-model](../topics/data-model.md)、
[storage-and-state](../../../baseline/storage-and-state.md)
