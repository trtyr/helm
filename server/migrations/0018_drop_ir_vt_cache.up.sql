-- 移除 VirusTotal 查杀缓存表（功能废弃：commit 27ca048，决策 S-011 配套的 VT 查杀随 owner 拍板移除）
DROP TABLE IF EXISTS ir_vt_cache;
