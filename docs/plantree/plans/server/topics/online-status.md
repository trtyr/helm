# 在线 / 离线状态

Role: topic-capsule
Status: active
Read when: 需要了解 Agent 在线状态的判定与 API 暴露
Related: [decisions/006](../decisions/006-online-status-detection.md)

## One-Screen Summary

「在线」= `ConnectionRegistry` 存在活跃连接（实时、内存）；「离线」= 无活跃连接且
`last_heartbeat_at` 超过阈值。API 暴露 `online`（实时）+ `last_seen`（持久化）两个字段。

## Current Position

已实现（Phase 5）：心跳超时判定 `is_stale(last_seen, now, timeout)`（阈值可配，默认 30s）；
`GET /api/v1/hosts` 返回 online / last_seen / stale。

## Active Constraints

- 离线阈值默认 3×心跳间隔（30s），可配置。
- 离线判定用轻量后台任务周期性扫描（或惰性计算），不阻塞主链路。
- 状态变更（上线/离线）产生事件，供告警/审计消费。
- Server 重启后「在线」归零（内存态），靠 `last_seen` 兜底展示。

## Open Risks Or Questions

- 大规模 agent 的在线状态聚合性能 → open-questions#10。

## Details

- 数据源：`agents.last_heartbeat_at`（持久化）+ `ConnectionRegistry`（内存）。
- 完整决策见 [decisions/006](../decisions/006-online-status-detection.md)。
