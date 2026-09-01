-- 监听器：Agent 接入点（可配置、可多开、可启停）。
-- auth 为空时回退到全局 HELM_SERVER_TOKEN。
CREATE TABLE listeners (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    addr TEXT NOT NULL,
    proto TEXT NOT NULL DEFAULT 'grpc' CHECK (proto IN ('grpc')),
    auth TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT 'stopped' CHECK (status IN ('running', 'stopped')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_listeners_status ON listeners(status);
