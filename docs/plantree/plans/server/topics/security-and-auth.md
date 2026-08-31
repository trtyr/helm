# 安全与认证

Role: topic-capsule
Status: planning
Read when: 需要了解 Agent 认证、TLS、权限、密钥管理
Related: [baseline risk-hotspots](../../../baseline/risk-hotspots.md)

## One-Screen Summary

底座含三层安全：传输加密（TLS）、身份认证（token）、访问控制（RBAC）。
密钥不硬编码，从环境变量/secret 注入。

## Current Position

设计阶段。起步水位：TLS + token；mTLS 与 RBAC 列为强化项（Phase 4）。

## Active Constraints

- Agent 注册携带 token，Server 校验后绑定 host。
- 命令执行是高风险面：倾向命令白名单 + 审计日志。
- 错误响应不泄漏内部信息（安全消息 + 关联 ID）。

## Open Risks Or Questions

- token 轮换与吊销机制何时做 → open-questions#6
- mTLS 是否提前（用户强调底座，可能希望更早）

## Details

- 风险缓解见 [baseline risk-hotspots](../../../baseline/risk-hotspots.md)
