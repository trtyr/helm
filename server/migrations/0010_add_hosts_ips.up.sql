-- hosts 加 IP 字段：public_ip 为 Server 看到的 Agent 连接源地址（外网视角），
-- local_ips 为 Agent 注册时上报的本机网卡地址（内网视角），IPv4 在前。
ALTER TABLE hosts ADD COLUMN public_ip TEXT NOT NULL DEFAULT '';
ALTER TABLE hosts ADD COLUMN local_ips TEXT[] NOT NULL DEFAULT '{}';
