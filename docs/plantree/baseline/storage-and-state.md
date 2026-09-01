# 存储与状态

Role: detail-shard（baseline 全局上下文）
Status: active

## 存储选型

- ORM/查询层：`sqlx`（异步、编译期 SQL 检查）。
- 数据库：**PostgreSQL**（dev/CI 用 docker-compose 起容器，宿主机 `5433`→容器 `5432`）。
- 决策见 [plans/server/decisions/004](../plans/server/decisions/004-storage-postgres-direct.md)（取代 003）。

## 实体模型（已落地）

权威 schema 见 [docs/data-model.md](../../data-model.md) 与 `server/migrations/`。下表为概览：

| 实体 | 关键字段 | 说明 |
|------|---------|------|
| Host | id, hostname, os, platform, tags, conn_mode, addr | 目标主机 |
| Agent | id, host_id, version, registered_at, last_heartbeat | Agent 实例/注册 |
| Task | id, name, kind(exec/file/script), params, timeout | 任务定义 |
| Job | id, task_id, host_id, status, output, exit_code | 执行实例 |
| FileTransfer | id, host_id, direction, path, size, status | 文件传输 |
| Metric | host_id, name, value, labels, ts | 状态指标时间序列 |
| User | id, username, role | 控制台用户 |

## 状态机

- **Job**：`queued → running → succeeded | failed | timed_out | cancelled`
- **FileTransfer**：`pending → transferring → done | failed`
- **Agent 在线**：由活跃连接推导（内存注册表 `ConnectionRegistry`），`last_heartbeat` 持久化。

## 迁移机制

- 用 `sqlx migrate`（`sqlx::migrate!("./migrations")`），迁移文件纳入版本控制，不可回改已发布迁移。
- schema 从一开始包含：外键、索引、时间戳、软删除标记。
