-- 状态事件（P003 T1）：agent 上下线/断连原因落库，供 /logs/events 查询
CREATE TABLE status_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id TEXT NOT NULL,
    event TEXT NOT NULL CHECK (event IN ('online', 'offline')),
    reason TEXT NOT NULL DEFAULT '',
    detail TEXT NOT NULL DEFAULT '',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_status_events_host_created ON status_events (host_id, created_at DESC);
