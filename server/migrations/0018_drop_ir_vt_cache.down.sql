-- 恢复 VirusTotal 查杀缓存表（仅回滚用；功能本身已废弃）
CREATE TABLE IF NOT EXISTS ir_vt_cache (
    sha256 TEXT PRIMARY KEY,
    positives INT NOT NULL,
    total INT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
