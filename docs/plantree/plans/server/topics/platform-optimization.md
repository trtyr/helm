# Linux / Windows 平台优化

Role: topic-capsule
Status: active
Read when: 需要了解 Agent 跨平台能力与各 OS 适配的规划
Related: [roadmap](../roadmap.md)、[decisions/008](../decisions/008-agent-persistence.md)

## One-Screen Summary

让 Agent 在 Linux / Windows 上各得其所：Windows 服务 + 去黑窗口 + ConPTY，Linux systemd + journald；
进程 / 文件 / 服务管理按平台适配。对应 Phase 7（服务化）+ Phase 6（平台相关能力）。

## Current Position

Agent 已是跨平台单二进制（`tokio` + `sysinfo` + `hostname`），但：

- Windows 以控制台运行（黑窗口）、无服务化、非管理员用户受限（实测）。
- Linux 无 systemd unit。
- 平台相关能力（进程 / 文件 / 服务）未做 OS 抽象层。

## Active Constraints

- Windows：`#![windows_subsystem = "windows"]` 去黑窗口 + 日志落文件 + 服务安装（nssm / `sc create`）。
- Linux：systemd unit + journald。
- 平台抽象层：进程 / 文件 / 服务 / PTY 各自按 `cfg(target_os)` 适配，Server 只发语义指令。
- 服务安装需管理员权限（当前非管理员，部署时提权）。

## Open Risks Or Questions

- Windows 非管理员下的部署策略（是否需要提权引导）。

## Details

- 持久化决策见 [decisions/008](../decisions/008-agent-persistence.md)。
- 完成标准见 [roadmap Phase 7](../roadmap.md)。
