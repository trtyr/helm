# 007 — 安全水位：mTLS + 审计（单用户，暂不 RBAC）

Date: 2026-09-01

## Context

当前明文 gRPC（无 TLS）、`users.role` 存了 admin/operator 但 API 不校验、
无审计日志。作为面向生产主机的运维平台，这是硬缺口（Phase 7）。

## Decision

- 传输层：Agent ↔ Server 上 **mTLS**（双向认证），控制台 HTTP 至少 TLS。
- 授权：**单用户，暂不 RBAC**。`users.role` 字段保留但不校验，等多人协作再上 RBAC。
- 审计：结构化审计日志记录关键操作（登录、命令下发、文件传输、主机增删、监听器启停）。

## Consequences

### 启用

- 传输加密 + 双向认证，防中间人；RBAC 落地；操作可追溯。

### 约束/代价

- 证书签发 / 分发 / 轮换机制需设计与实现。
- RBAC 中间件 + 审计存储带来额外复杂度。

**相关：** [topics/security-hardening](../topics/security-hardening.md)
