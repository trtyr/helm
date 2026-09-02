-- 告警记录表
CREATE TABLE alerts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES hosts(id) ON DELETE CASCADE,
    metric_name TEXT NOT NULL,
    threshold FLOAT8 NOT NULL,
    value FLOAT8 NOT NULL,
    level TEXT NOT NULL DEFAULT 'warning',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_alerts_created ON alerts(created_at DESC);
