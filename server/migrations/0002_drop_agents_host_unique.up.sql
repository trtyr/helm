-- 移除 agents.host_id 的唯一约束：一个 host 允许存在多个 agent 记录
--（测试/重装场景下同一台机器会以不同 agent_id 多次注册）。
ALTER TABLE agents DROP CONSTRAINT IF EXISTS agents_host_id_key;
