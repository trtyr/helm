# 测试与发布门禁

Role: detail-shard（baseline 全局上下文）
Status: planning

## 测试分层

| 层 | 范围 | 工具 |
|----|------|------|
| 单元测试 | domain/application 纯逻辑 | `#[cfg(test)]` |
| 集成测试 | store 仓储 + sqlx | `tests/` + 内存/临时 SQLite |
| 契约测试 | protobuf 服务定义一致性 | buf + grpcurl |
| 端到端 | 一条链路：Agent 连入 → 下发命令 → 拿回输出 | 本地 fixture |

## 发布门禁（合并前必须通过）

1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test`（单元 + 集成）
4. protobuf 契约兼容性检查（`buf breaking`，避免破坏 Agent 兼容）
5. 无密钥/敏感信息泄漏进 diff

## 契约演进规则

- protobuf 只做向后兼容变更（新增字段/服务，不删不改编号）。
- 破坏性变更走 `v2` 服务版本，旧版本保留一个过渡期。
