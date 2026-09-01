-- 常驻服务/后台任务：在目标主机上托管长跑进程并持续监听。
CREATE TABLE services (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    command TEXT NOT NULL,
    args TEXT[] NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'stopped' CHECK (status IN ('running', 'stopped', 'failed')),
    restart_policy TEXT NOT NULL DEFAULT 'no' CHECK (restart_policy IN ('no', 'always')),
    pid INTEGER,
    exit_code INTEGER,
    log TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_services_host ON services(host_id);
