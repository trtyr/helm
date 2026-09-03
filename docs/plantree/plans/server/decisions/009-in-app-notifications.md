# 009 — 通知为系统内部能力（小卡片），不做外发

Date: 2026-09-03

## Context

Phase 7 落了阈值预警（`alerts` 表 + `GET /api/v1/alerts`），但「通知」的形态一直没有明确定义，
讨论中曾把外发通道（webhook / 邮件 / 钉钉）当作告警的自然延伸方向。

2026-09-03 用户明确定义纠正：通知是**系统内部的小卡片消息**——呈现给正在看控制台的人，
内容是主机上线、下线、以及预警这类事件；向外推送不是本项目的目标。
对照发现现有能力矩阵把「通知」窄化成了「阈值告警」，且上线/下线事件（`ConnectionRegistry`
的 register/unregister）只进日志、不落库不推送，前端根本拿不到。

## Decision

- **通知 = 系统内部能力**：站内消息（前端小卡片 / 消息中心形态），**不做外发**
  （webhook / 邮件 / 钉钉均不在范围，除非未来另有决策）。
- **通知内容三类**：`online`（主机上线）/ `offline`（主机下线）/ `alert`（预警联动）。
  资源阈值预警这类低频事件对通知的价值有限，通知的主体是上/下线这类状态变化。
- **数据模型**：新增 `notifications` 表做统一通知中心；`alerts` 表保留，继续承载
  阈值预警的时序历史（语义不变），预警落库时联动生成一条 `alert` 类型通知。
- **已读/未读**：通知带 read 状态（小卡片红点 / 铃铛场景的标配）。

## Consequences

### 启用

- 通知有了清晰边界与形态，不再与「外发告警」混淆。
- 前端小卡片有完整后端支撑：通知列表 / 未读数 / 标记已读 / 实时推送。
- 上/下线事件从「只进日志」变成对控制台使用者可见的一等公民。

### 约束/代价

- 上线/下线事件需在连接注册/注销链路埋点（`ConnectionRegistry` register/unregister、
  `InboundCtx::on_disconnect`、心跳超时判定）。
- 频繁重连 / Server 重启风暴可能产生通知刷屏，实现时需考虑去重或冷却窗口（见
  [topics/notifications](../topics/notifications.md) 开放问题）。

**相关：** [006 在线判定](006-online-status-detection.md)（上/下线信号源）、
[topics/extended-monitoring](../topics/extended-monitoring.md)、
[topics/notifications](../topics/notifications.md)
