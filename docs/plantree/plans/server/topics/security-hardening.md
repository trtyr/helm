# 安全强化

Role: topic-capsule
Status: planning
Read when: 需要了解 mTLS / RBAC / 审计 / 白名单的规划
Related: [decisions/007](../decisions/007-security-posture.md)

## One-Screen Summary

补齐安全缺口：Agent ↔ Server 上 mTLS（双向认证）、HTTP 层按 role 强制 RBAC、
结构化审计日志、命令白名单/最小权限。对应 Phase 7。

## Current Position

已有：Agent token 严格认证、控制台 JWT + bcrypt、单点错误边界（不外泄内部串）。
缺失：明文 gRPC（无 TLS）、RBAC 存而不查（`users.role` 不校验）、无审计、无白名单。
规划中，未实现。

## Active Constraints

- 传输层 mTLS：Agent 与 Server 双向认证；控制台 HTTP 至少 TLS。
- RBAC 复用现有 `Claims.role`（admin 全量 / operator 受限），不另起权限模型。
- 审计记录关键操作：登录、命令下发、文件传输、主机增删、监听器启停。
- 错误响应继续走单点边界，安全消息不泄漏内部细节。

## Open Risks Or Questions

- mTLS 证书签发/分发/轮换机制 → open-questions#9。

## Details

- 完整决策见 [decisions/007](../decisions/007-security-posture.md)。
