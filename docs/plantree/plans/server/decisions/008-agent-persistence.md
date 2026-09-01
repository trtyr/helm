# 008 — Agent 持久化：服务化 + 去黑窗口

Date: 2026-09-01

## Context

Agent 目前是控制台程序：Windows 上运行时弹黑窗口，且无开机自启；
Linux 无 systemd unit。作为 C2 驻留能力，Agent 需要静默、常驻、自启（Phase 7）。

## Decision

- Windows：二进制编译为 GUI 子系统（`#![windows_subsystem = "windows"]`）去黑窗口，
  日志落文件；以 Windows 服务（nssm / `sc create`）安装，开机自启。
- Linux：提供 systemd unit，开机自启，日志走 journald 或文件。
- 日志：无控制台后，tracing 输出重定向到文件（按天滚动）。

## Consequences

### 启用

- Agent 静默常驻、开机自启，接近 C2 的「驻留」语义。

### 约束/代价

- Windows 服务安装需管理员权限（与「非管理员用户」现状冲突，需部署时提权）。
- 日志从 stdout 迁文件，需日志轮转与清理策略。

**相关：** [topics/agent-persistence](../topics/agent-persistence.md)
