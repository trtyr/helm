# 存储与状态

Role: detail-shard（baseline 全局上下文）
Status: planning

## 存储选型

- ORM/查询层：`sqlx`（异步、编译期检查、多数据库抽象）。
- 起步数据库：SQLite（MVP 零部署），schema 按生产标准设计。
- 目标数据库：PostgreSQL，切换仅改连接串 + 迁移方言。
- 决策见 [plans/server/decisions/003](../plans/server/decisions/003-storage-sqlx-sqlite-first.md)。

## 实体模型（初版）

| 实体 | 关键字段 | 说明 |
|------|---------|------|
| Host | id, hostname, os, platform, labels, conn_mode | 目标主机 |
| Agent | id, host_id, version, registered_at, last_heartbeat | Agent 实例/注册 |
| Task | id, name, kind(exec/file/script), params, timeout | 任务定义 |
| Job | id, task_id, host_id, status, output, exit_code | 执行实例 |
| FileTransfer | id, host_id, direction, path, size, status | 文件传输 |
| Metric | host_id, ts, kind, value | 状态指标时间序列 |
| User | id, username, role | 控制台用户 |

## 状态机

- **Job**：`queued → running → succeeded | failed | timed_out | cancelled`
- **FileTransfer**：`pending → transferring → done | failed | cancelled`
- **Agent 在线**：由活跃连接推导（内存注册表），`last_heartbeat` 持久化。

## 迁移机制

- 用 `sqlx migrate`，迁移文件纳入版本控制，不可回改已发布迁移。
- schema 从一开始包含：外键、索引、时间戳、软删除标记。
