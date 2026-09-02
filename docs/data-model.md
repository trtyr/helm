# Data Model — 存储与实体模型

数据库：**PostgreSQL**（`docker-compose.yml` 起容器，宿主机端口 5433→容器 5432）。
ORM/查询层：**sqlx**（异步、编译期 SQL 检查）。迁移用 `sqlx::migrate!("./migrations")` 在 Server 启动时执行。

## 迁移版本

| 版本 | 文件 | 内容 |
|------|------|------|
| 1 | `0001_init.up.sql` | 初始 schema：hosts / agents / tasks / jobs / file_transfers / metrics / users |
| 2 | `0002_drop_agents_host_unique.up.sql` | 移除 `agents.host_id` 唯一约束（一台机器可多次注册不同 agent_id） |
| 3 | `0003_add_hosts_addr.up.sql` | `hosts` 加 `addr`（forward 模式拨号地址） |
| 4 | `0004_add_listeners.up.sql` | `listeners` 表（gRPC 监听器） |
| 5 | `0005_add_services.up.sql` | `services` 表（常驻服务） |
| 6 | `0006_add_audit_logs.up.sql` | `audit_logs` 表（审计日志） |
| 7 | `0007_add_alerts.up.sql` | `alerts` 表（阈值告警） |

> 每个迁移都有对应 `.down.sql` 回滚文件。约定：所有表含 `created_at`/`updated_at`，外键 + 索引，
> 软删除标记 `deleted_at`（hosts/users/tasks/jobs/file_transfers/metrics；`agents`/`listeners`/`services`/`audit_logs`/`alerts` 无软删除）。

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

> 迁移 2 后 `host_id` 不再唯一：同一 host 可有多条 agent 记录。`agents` 表无 `deleted_at`，
> `last_heartbeat_at` 可空（`AgentRow` 对应 `Option<DateTime<Utc>>`）。

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

### `listeners` — gRPC 监听器（迁移 4）

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | `gen_random_uuid()` |
| name | TEXT | 监听器名 |
| addr | TEXT | 绑定地址 |
| proto | TEXT | `grpc`（CHECK） |
| auth | TEXT | 监听器专用 token（空则回退全局 `HELM_SERVER_TOKEN`） |
| status | TEXT | `running` / `stopped`（CHECK） |
| created_at / updated_at | TIMESTAMPTZ | 时间戳 |

索引：`idx_listeners_status(status)`。

### `services` — 常驻服务（迁移 5）

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | `gen_random_uuid()` |
| host_id | UUID FK → hosts | `ON DELETE CASCADE` |
| name | TEXT | 服务名 |
| command | TEXT | 命令 |
| args | TEXT[] | 参数，默认 `{}` |
| status | TEXT | `running` / `stopped` / `failed`（CHECK），默认 `stopped` |
| restart_policy | TEXT | `no` / `always`（CHECK），默认 `no` |
| pid | INTEGER | 运行 PID |
| exit_code | INTEGER | 退出码 |
| log | TEXT | 增量日志 |
| created_at / updated_at | TIMESTAMPTZ | 时间戳 |

索引：`idx_services_host(host_id)`。

### `audit_logs` — 审计日志（迁移 6）

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | `gen_random_uuid()` |
| actor | TEXT | 操作者 |
| action | TEXT | 动作 |
| resource | TEXT | 资源 |
| detail | JSONB | 详情 |
| created_at | TIMESTAMPTZ | 时间 |

索引：`idx_audit_logs_created(created_at DESC)`。

### `alerts` — 阈值告警（迁移 7）

| 字段 | 类型 | 说明 |
|------|------|------|
| id | UUID PK | `gen_random_uuid()` |
| host_id | UUID FK → hosts | `ON DELETE CASCADE` |
| metric_name | TEXT | 指标名 |
| threshold | FLOAT8 | 阈值 |
| value | FLOAT8 | 实测值 |
| level | TEXT | 告警级别，默认 `warning` |
| created_at | TIMESTAMPTZ | 时间 |

索引：`idx_alerts_created(created_at DESC)`。

## 状态机

| 实体 | 状态流转 | 终态判断 |
|------|---------|---------|
| Job | `queued → running → succeeded \| failed \| timed_out \| cancelled` | `domain::job::JobStatus::is_terminal()` |
| FileTransfer | `pending → transferring → done \| failed` | — |
| Listener | `stopped ↔ running`（动态启停） | — |
| Service | `stopped → running → stopped \| failed` | restart_policy=always 时 failed 自动重启 |
| Agent 在线 | 内存注册表推导（`ConnectionRegistry`），`last_heartbeat_at` 持久化 + `is_stale` 判定 | — |

Job 终态由 `grpc::agent_service::job_status(error, exit_code)` 判定：有 error → failed；exit_code 0/None → succeeded；其余 → failed。
Service 状态映射由 `agent_service::map_service_status`：running→running，failed→failed，exited→stopped，未知→None。

## 数据流

- **Agent 注册**：`agent_service.rs` 收到 `Register` → `AgentRepo::register`（事务：按 hostname 复用或新建 host + upsert agent）→ 返回 host_id 供后续指标/心跳落库关联。
- **命令结果**：Agent `ExecResult(finished)` → `JobRepo::finish(id, status, output, exit_code)`。
- **指标**：Agent `MetricReport` → `MetricRepo::insert(host_id, name, value, ts)`，逐条落库；超阈值同时落 `alerts`。
- **文件**：`FileService` 建 `FileTransferRepo::create` → 传输完成 `finish(status, bytes, checksum)`。
- **服务日志**：Agent `ServiceStatus(log)` → `ServiceRepo::append_log` 追加。
- **审计**：关键端点（login/exec/文件/主机/监听器）→ `AuditRepo::insert(actor, action, resource, detail)`。
- **时序保留**：后台任务每 24h 清理 30 天前的 `metrics` 与 `alerts`（`delete_before`）。

仓储层实现见 `server/src/store/*_repo.rs`；相关规划稿见 [docs/plantree/baseline/storage-and-state.md](plantree/baseline/storage-and-state.md)（注意其中「SQLite 起步」已被决策 004 取代）。
