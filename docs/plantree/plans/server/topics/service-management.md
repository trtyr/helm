# 服务管理（持久化后台任务）

Role: topic-capsule
Status: active
Read when: 需要了解「在 Agent 上跑常驻任务并持续监听」的规划
Related: [roadmap](../roadmap.md)

## One-Screen Summary

在目标主机上部署一个**持久化后台任务 / 服务**（类似 systemd service / supervisor / pm2 / BG 工具），
并持续监听它的状态、日志，支持启动 / 停止 / 重启。对应 Phase 6。

## Current Position

已实现（Phase 6）：常驻服务 `services` 表 + `ServiceManager`（启动/停止/重启/日志 + restart_policy）。
HTTP：`/api/v1/services` CRUD + start/stop/restart/logs + logs/stream 实时流。

## Active Constraints

- 服务/任务定义落库（`tasks` 表已有，扩展 kind 与 params 表达「常驻服务」）。
- Agent 端托管子进程：启动 / 停止 / 重启 / 采集状态（running/exited）+ 日志环形缓冲。
- 状态与日志经双向流上报，控制台经 WebSocket 订阅（`tail -f` 那类实时日志）。
- 崩溃自动重启（restart policy）可选。

## Open Risks Or Questions

- 日志保留策略（环形缓冲大小 / 是否落盘）。
- 已定：新增 `services` 表（migration 0005）。

## Details

- 完成标准见 [roadmap Phase 6](../roadmap.md)。
