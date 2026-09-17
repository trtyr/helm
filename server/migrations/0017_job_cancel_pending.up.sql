-- EN-64：掉线期间的 job 取消补偿（模式对照 agent_pending_offline）。
-- cancel 一个 running 中的 job 但 agent 离线时：job 立即置 cancelled（终态），
-- 同时记一条补偿；agent 重连后补发 JobCancel 杀掉目标机上的残留进程。
CREATE TABLE job_cancel_pending (
    agent_id TEXT NOT NULL,
    job_id UUID NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_id, job_id)
);
