-- 掉线期间挂起的下线/注销：agent 重连瞬间补执行（解决"下线了但进程还在"）。
CREATE TABLE IF NOT EXISTS agent_pending_offline (
    agent_id TEXT PRIMARY KEY,
    action TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
