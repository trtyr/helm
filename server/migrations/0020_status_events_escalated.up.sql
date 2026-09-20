-- 离线升级告警标记（P003 T2）：offline 事件升级为 alerts 后打标，防重复触发
ALTER TABLE status_events ADD COLUMN escalated BOOLEAN NOT NULL DEFAULT false;
