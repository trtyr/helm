-- 回滚 0003：移除 hosts.addr。
ALTER TABLE hosts DROP COLUMN IF EXISTS addr;
