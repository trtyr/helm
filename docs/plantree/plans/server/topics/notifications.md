# 通知中心

Role: topic-capsule
Status: active
Read when: 需要了解系统内通知（上线/下线/预警小卡片）的设计、数据模型与落地细节
Related: [roadmap](../roadmap.md)、[决策 009](../decisions/009-in-app-notifications.md)

## One-Screen Summary

通知 = 系统内部小卡片消息，内容为主机**上线 / 下线 / 预警**三类，带已读/未读，
不做外发（决策 009）。`notifications` 表做统一通知中心；`alerts` 保留预警时序历史，
预警落库时联动生成通知。Phase 9 已落地。

## User Intent（2026-09-03，决策 009）

通知呈现给正在看控制台的人（前端小卡片 / 消息中心），**不是向外推**。
资源阈值类预警对通知价值有限，通知的主体是上/下线这类状态变化。

## Current Position（Phase 9 已实现）

- 数据层：`notifications` 表（migration 0008：type CHECK online/offline/alert + read 布尔）
  - `store/notification_repo.rs`（分页/未读过滤/单条与全部已读/冷却查询/保留清理）。
- 用例层：`application/notification_service.rs` —— `notify()`（冷却判定 → 合并刷新或插入 →
  StreamRegistry 广播）、查询/已读方法、`spawn_offline_sweeper` 兜底扫描。
- 埋点：`agent_service.rs`（reverse 注册）与 `forward_manager.rs`（forward 注册）发 online；
  `InboundCtx::on_disconnect` 发 offline（断连即发）；`InboundCtx` 指标超阈值发 alert（预警联动）；
  兜底扫描对半开死连接注销并补发（只对刚进入 stale 的）。
- HTTP：`GET /notifications`（分页 + unread 过滤）/ `unread-count` / `{id}/read` / `read-all`；
  WS `/notifications/stream` 实时推送。
- 保留：30 天后台清理（与 metrics/alerts 同一循环）。

## 已决口径（原开放问题）

- **下线判定**：断连即发（`on_disconnect`）+ 心跳超时兜底扫描
  （`should_notify_stale`：仅 `timeout ≤ age < 2×timeout` 刚进入 stale 的补发；
  半开连接由 sweeper 注销）。
- **冷却窗口**：固定 5 分钟（`COOLDOWN_SECS = 300`），同 host 同类型合并为一条
  （刷新消息与时间、重置未读）；不做配置面。

## Active Constraints

- 通知是系统内部能力，不外发（决策 009）。
- `alerts` 保留预警时序历史语义；`notifications` 只管「给人看的消息」，两者不混表。
- 实时推送复用 `StreamRegistry` 模式（key = `notifications`）。

## Details

- 落地证据与验收：[roadmap Phase 9](../roadmap.md)（e2e-phase9.py 五段 + 63 测试 + openapi 44 端点）。
