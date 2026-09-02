# 会话 / 交互终端

Role: topic-capsule
Status: active
Read when: 需要了解实时交互终端（SSH 终端那类）的规划
Related: [roadmap](../roadmap.md)

## One-Screen Summary

在平台上开一个到目标主机的**实时交互终端**（类似 SSH / Xshell），经 WebSocket 实时双向流 + PTY（伪终端）承载输入输出。对应 Phase 6。

## Current Position

仅命令执行（`/api/v1/exec`，一次性、非交互）。无交互终端。规划中，未实现。

## Active Constraints

- 控制台 ↔ Server 用 WebSocket（实时双向）；Server ↔ Agent 复用双向流底座。
- Agent 端需 PTY 分配：Linux 用 `nix`/`tokio` 的 pty，Windows 用 ConPTY（Windows 伪终端）。
- 会话有生命周期：建立 / 输入 / 输出 / 关闭 / 超时，会话可多开。

## Open Risks Or Questions

- Windows ConPTY 的复杂度（见 open-questions#7）。
- 会话是否需要录制/审计回放（当前 Deferred）。

## Details

- 完成标准见 [roadmap Phase 6](../roadmap.md)。
