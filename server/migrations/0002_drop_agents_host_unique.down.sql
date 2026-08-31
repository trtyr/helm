-- 回滚 0002：恢复 agents.host_id 的唯一约束。
ALTER TABLE agents ADD CONSTRAINT agents_host_id_key UNIQUE (host_id);
