# 监控扩展

Role: topic-capsule
Status: active
Read when: 需要了解指标采集扩展（磁盘 / 网络 / 进程 / 告警 / 保留）的规划
Related: [roadmap](../roadmap.md)

## One-Screen Summary

在现有 cpu/mem/proc 指标之上，扩展磁盘/网络/进程列表采集，加指标告警与时序保留策略。
对应 Phase 7。

## Current Position

已实现（Phase 7）：`agent/src/monitor.rs` 每 30s 采集 cpu/mem/disk.usage/net.rx/tx/proc.count，
经 `MetricReport` 上报落库；阈值告警落 `alerts` 表 + `GET /api/v1/alerts`；时序保留 30 天（后台清理）。

## Active Constraints

- 复用现有 `MetricReport` 信令与 `metrics` 表，扩展指标名与 labels。
- 时序保留：`metrics` 表加分区/滚动清理，避免无限增长（现有 risk-hotspots 已标）。
- 告警在 Server 侧做（阈值 + 通知），不把告警逻辑塞进 Agent。

## Open Risks Or Questions

- 大规模指标是否需要独立时序库（现 risk-hotspots 标为 Deferred）。

## Details

- 完成标准见 [roadmap Phase 7](../roadmap.md)。
