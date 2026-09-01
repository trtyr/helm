# Data Model — 存储与实体模型

数据库：**PostgreSQL**（`docker-compose.yml` 起容器，宿主机端口 5433→容器 5432）。
ORM/查询层：**sqlx**（异步、编译期 SQL 检查）。迁移用 `sqlx::migrate!("./migrations")` 在 Server 启动时执行。

## 迁移版本

| 版本 | 文件 | 内容 |
|------|------|------|
| 1 | `0001_init.up.sql` | 初始 schema：hosts / agents / tasks / jobs / file_transfers / metrics / users |
| 2 | `0002_drop_agents_host_unique.up.sql` | 移除 `agents.host_id` 唯一约束（一台机器可多次注册不同 agent_id） |
| 3 | `0003_add_hosts_addr.up.sql` | `hosts` 加 `addr`（forward 模式拨号地址） |

> 每个迁移都有对应 `.down.sql` 回滚文件。约定：所有表含 `created_at`/`updated_at`，外键 + 索引，软删除标记 `deleted_at`。

## 实体表

### `hosts` — 目标主机

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | `gen_random_uuid()` |
| hostname | TEXT | 主机名 |
| os / arch / platform | TEXT | 系统信息 |
| tags | TEXT[] | 标签，默认 `{}` |
| conn_mode | TEXT | `reverse` / `forward`（CHECK） |
| addr | TEXT | forward 拨号地址（reverse 为空，迁移 3 加入） |
| created_at / updated_at / deleted_at | TIMESTAMPTZ | 时间戳 + 软删除 |

### `agents` — Agent 实例（注册记录）

| 字段 | 类型 | 说明 |
|------|------|------|
| id | TEXT PK | agent_id |
| host_id | UUID FK → hosts | `ON DELETE CASCADE` |
| version | TEXT | Agent 版本 |
| registered_at / last_heartbeat_at | TIMESTAMPTZ | 注册 / 心跳时间 |

> 迁移 2 后 `host_id` 不再唯一：同一 host 可有多条 agent 记录。

### `tasks` — 任务定义

`id` UUID PK，`name` TEXT，`kind` TEXT CHECK（exec/file/script），`params` JSONB，`timeout_secs` INTEGER，时间戳 + 软删除。

定时任务参数约定（`scheduler::parse_schedule_params` 解析）：`params = {agent_id, command, args[], interval_secs}`。

### `jobs` — 任务执行实例

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | job_id |
| task_id | UUID FK → tasks | `ON DELETE SET NULL`（可空） |
| host_id | UUID FK → hosts | `ON DELETE CASCADE` |
| status | TEXT | CHECK：queued/running/succeeded/failed/timed_out/cancelled |
| command / args | TEXT / TEXT[] | 命令与参数 |
| output / exit_code | TEXT / INTEGER | 执行结果 |
| started_at / finished_at | TIMESTAMPTZ | 起止时间 |

索引：`idx_jobs_host_created(host_id, created_at DESC)`、`idx_jobs_status(status)`。

### `file_transfers` — 文件传输

`id` UUID PK，`host_id` FK → hosts，`direction` TEXT CHECK（upload/download），`path` TEXT，
`size`/`bytes_transferred` BIGINT，`checksum` TEXT，`status` TEXT CHECK（pending/transferring/done/failed），时间戳。
索引：`idx_file_transfers_host(host_id, created_at DESC)`。

### `metrics` — 状态指标（时间序列）

`host_id` FK → hosts，`name` TEXT，`value` DOUBLE PRECISION，`labels` JSONB，`ts` TIMESTAMPTZ。
索引：`idx_metrics_host_ts(host_id, ts DESC)`。

### `users` — 控制台用户

`id` UUID PK，`username` TEXT UNIQUE，`password_hash` TEXT，`role` TEXT CHECK（admin/operator），时间戳 + 软删除。

## 状态机

| 实体 | 状态流转 | 终态判断 |
|------|---------|---------|
| Job | `queued → running → succeeded \| failed \| timed_out \| cancelled` | `domain::job::JobStatus::is_terminal()` |
| FileTransfer | `pending → transferring → done \| failed` | — |
| Agent 在线 | 内存注册表推导（`ConnectionRegistry`），`last_heartbeat_at` 持久化 | — |

Job 终态由 `grpc::agent_service::job_status(error, exit_code)` 判定：有 error → failed；exit_code 0/None → succeeded；其余 → failed。

## 数据流

- **Agent 注册**：`agent_service.rs` 收到 `Register` → `AgentRepo::register`（事务：按 hostname 复用或新建 host + upsert agent）→ 返回 host_id 供后续指标/心跳落库关联。
- **命令结果**：Agent `ExecResult(finished)` → `JobRepo::finish(id, status, output, exit_code)`。
- **指标**：Agent `MetricReport` → `MetricRepo::insert(host_id, name, value, ts)`，逐条落库。
- **文件**：`FileService` 建 `FileTransferRepo::create` → 传输完成 `finish(status, bytes, checksum)`。

仓储层实现见 `server/src/store/*_repo.rs`；相关规划稿见 [docs/plantree/baseline/storage-and-state.md](plantree/baseline/storage-and-state.md)（注意其中「SQLite 起步」已被决策 004 取代）。
