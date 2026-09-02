# 控制能力

Role: topic-capsule
Status: active
Read when: 需要了解 C2 控制能力（shell / 进程 / 网络 / 文件系统 / 分组）的规划
Related: [roadmap](../roadmap.md)

## One-Screen Summary

在现有命令/文件/脚本/定时之上，补齐 C2 的「控制」能力：交互 shell、进程管理（list/kill）、
网络信息采集、文件系统浏览、Agent 分组/标签管理。对应 Phase 6。

## Current Position

已实现（Phase 6）：命令执行、文件上下传/列目录、脚本/定时之外，补齐交互 shell（WS+PTY）、
进程管理（list/kill）、网络信息、文件系统浏览、Agent 分组/标签。

## Active Constraints

- Server 发「语义指令」，不关心平台实现差异（沿用现有信令语义化原则）。
- 交互 shell 走 WebSocket 实时双向流 + PTY（伪终端），复用双向流底座。
- 进程/网络/文件系统能力经 Agent 端新增模块（如 `proc.rs` / `net.rs` / `fs.rs`）实现。

## Open Risks Or Questions

- 交互 shell 的 PTY 方案（Windows 伪终端是难点）→ open-questions#7。

## Details

- 完成标准见 [roadmap Phase 6](../roadmap.md)。
