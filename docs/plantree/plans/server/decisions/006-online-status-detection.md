# 006 — 在线判定：实时注册表 + 心跳超时

Date: 2026-09-01

## Context

心跳（`last_heartbeat_at`）与内存注册表（`ConnectionRegistry`）都已存在，
但没有「超时判离线」逻辑，也没有 API 暴露在线状态。C2 平台需要可靠的
「在线 / 离线 / 最后活跃」三态。

## Decision

- 「在线」= `ConnectionRegistry` 存在活跃连接（实时、内存）。
- 「离线」= 无活跃连接，且 `last_heartbeat_at` 超过阈值（默认 3×心跳间隔，可配）。
- API 暴露两个字段：`online`（实时 bool）+ `last_seen`（持久化的最后心跳时间）。
- 离线判定由一个轻量后台任务周期性扫描（或惰性计算），不阻塞主链路。

## Consequences

### 启用

- 控制台可实时感知 Agent 在线状态，支撑后续控制能力的「可达性」判断。
- 「实时在线」与「持久化最后活跃」分离，语义清晰。

### 约束/代价

- 需引入心跳超时阈值配置与离线判定任务。
- 注册表是内存态，Server 重启后「在线」归零，需靠 `last_seen` 兜底展示。

**相关：** [005 监听器](005-listener-model.md)、
[topics/online-status](../topics/online-status.md)
