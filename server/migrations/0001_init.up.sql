-- 初始 schema：核心实体表。
-- 约定：所有表含 created_at/updated_at；外键 + 索引；软删除标记 deleted_at。
-- 目标数据库 PostgreSQL（sqlx 迁移），本文件为版本 1。

-- 主机（目标机器）
CREATE TABLE hosts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hostname TEXT NOT NULL,
    os TEXT NOT NULL,
    arch TEXT NOT NULL,
    platform TEXT NOT NULL,
    tags TEXT[] NOT NULL DEFAULT '{}',
    conn_mode TEXT NOT NULL DEFAULT 'reverse' CHECK (conn_mode IN ('reverse', 'forward')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

-- Agent 实例（注册记录）
CREATE TABLE agents (
    id TEXT PRIMARY KEY,
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    version TEXT NOT NULL,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_heartbeat_at TIMESTAMPTZ,
    UNIQUE (host_id)
);

-- 任务定义（命令/脚本模板）
CREATE TABLE tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('exec', 'file', 'script')),
    params JSONB NOT NULL DEFAULT '{}',
    timeout_secs INTEGER,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);

-- 任务执行实例
CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    task_id UUID REFERENCES tasks(id) ON DELETE SET NULL,
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'succeeded', 'failed', 'timed_out', 'cancelled')),
    command TEXT NOT NULL,
    args TEXT[] NOT NULL DEFAULT '{}',
    output TEXT,
    exit_code INTEGER,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_jobs_host_created ON jobs(host_id, created_at DESC);
CREATE INDEX idx_jobs_status ON jobs(status);

-- 文件传输
CREATE TABLE file_transfers (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    direction TEXT NOT NULL CHECK (direction IN ('upload', 'download')),
    path TEXT NOT NULL,
    size BIGINT NOT NULL DEFAULT 0,
    bytes_transferred BIGINT NOT NULL DEFAULT 0,
    checksum TEXT,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'transferring', 'done', 'failed')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    finished_at TIMESTAMPTZ
);
CREATE INDEX idx_file_transfers_host ON file_transfers(host_id, created_at DESC);

-- 状态指标（时间序列）
CREATE TABLE metrics (
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    value DOUBLE PRECISION NOT NULL,
    labels JSONB NOT NULL DEFAULT '{}',
    ts TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_metrics_host_ts ON metrics(host_id, ts DESC);

-- 控制台用户
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role TEXT NOT NULL DEFAULT 'operator' CHECK (role IN ('admin', 'operator')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ
);
