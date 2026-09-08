-- IR 页面缓存：自启动项/系统日志等扫描结果的"最后一次扫描"快照，页面打开秒显。
CREATE TABLE IF NOT EXISTS ir_page_cache (
    agent_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    findings JSONB NOT NULL DEFAULT '[]',
    entry_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, kind)
);
