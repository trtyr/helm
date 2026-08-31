# 004 — 存储：直接上 Postgres（取代 003）

Date: 2026-08-31

## Context

用户确认 goal 范围时明确选择「直接上 Postgres，dev/CI 都起容器」，
强调底座一步到位、贴近生产，不因 MVP 在 SQLite 上妥协。

## Decision

- 持久层用 **sqlx**（异步、编译期 SQL 检查）。
- 数据库直接使用 **PostgreSQL**，dev/CI 均通过 docker-compose 起容器。
- 不再使用 SQLite。

## Consequences

### 启用

- schema、迁移、索引、外键直接在 Postgres 上按生产标准落地，无方言迁移风险。
- dev/CI 与生产同库，避免「SQLite 跑得通、Postgres 报错」的隐藏差异。

### 约束/代价

- 本地开发需起 Postgres 容器（docker-compose 一键）。
- CI 需 Postgres 服务依赖。

## Supersedes

[003](003-storage-sqlx-sqlite-first.md)
