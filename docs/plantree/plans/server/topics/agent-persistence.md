# Agent 持久化

Role: topic-capsule
Status: active
Read when: 需要了解 Agent 服务化 / 去黑窗口 / 开机自启的规划
Related: [decisions/008](../decisions/008-agent-persistence.md)

## One-Screen Summary

Agent 从控制台程序升级为静默常驻的服务：Windows 去黑窗口（GUI 子系统）+ 服务安装；
Linux 用 systemd unit；日志落文件。对应 Phase 7。

## Current Position

Agent 是控制台程序：Windows 运行时弹黑窗口、无开机自启；Linux 无 systemd unit。
规划中，未实现。

## Active Constraints

- Windows 二进制编译 `#![windows_subsystem = "windows"]` 去黑窗口，日志重定向到文件（按天滚动）。
- Windows 服务用 nssm 或 `sc create` 安装；Linux 提供 systemd unit。
- 无控制台后，tracing 输出从 stdout 迁文件，需日志轮转/清理策略。
- 服务安装需管理员权限（与当前非管理员用户现状冲突，部署时提权）。

## Open Risks Or Questions

- 日志轮转与清理策略的具体参数（大小/天数）。

## Details

- 完整决策见 [decisions/008](../decisions/008-agent-persistence.md)。
