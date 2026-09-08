-- IR 启动项基线快照（Autoruns 基线对比工作流）+ VirusTotal 查杀缓存。
CREATE TABLE IF NOT EXISTS ir_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',
    findings JSONB NOT NULL DEFAULT '[]',
    entry_count INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS ir_snapshots_agent_idx ON ir_snapshots (agent_id, created_at DESC);

CREATE TABLE IF NOT EXISTS ir_vt_cache (
    sha256 TEXT PRIMARY KEY,
    positives INT NOT NULL,
    total INT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
