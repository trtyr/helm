-- agent 权限标识：注册时上报是否以管理员/root 运行。
ALTER TABLE agents ADD COLUMN elevated BOOLEAN NOT NULL DEFAULT FALSE;
