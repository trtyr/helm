# 决策索引

Role: index
Status: active

| # | 决策 | 状态 | 主题 |
|---|------|------|------|
| [001](001-grpc-as-communication-base.md) | 通信底座用 gRPC（非 WebSocket+JSON） | active | communication-protocol |
| [002](002-bidirectional-stream-connection-model.md) | 双向流统一正/反向连接模型 | active | connection-model |
| [003](003-storage-sqlx-sqlite-first.md) | 存储：sqlx + SQLite 起步，Postgres 目标 | superseded | data-model |
| [004](004-storage-postgres-direct.md) | 存储：直接上 Postgres（取代 003） | active | data-model |
| [005](005-listener-model.md) | 监听器作为一等公民（DB 实体 + API 管理） | active | listener-management |
| [006](006-online-status-detection.md) | 在线判定：实时注册表 + 心跳超时 | active | online-status |
| [007](007-security-posture.md) | 安全水位：mTLS + RBAC 强制 + 审计 | active | security-hardening |
| [008](008-agent-persistence.md) | Agent 持久化：服务化 + 去黑窗口 | active | agent-persistence |

## 主题索引

- communication-protocol → [001](001-grpc-as-communication-base.md)
- connection-model → [002](002-bidirectional-stream-connection-model.md)
- data-model → [004](004-storage-postgres-direct.md)（003 已取代）
- listener-management → [005](005-listener-model.md)
- online-status → [006](006-online-status-detection.md)
- security-hardening → [007](007-security-posture.md)
- agent-persistence → [008](008-agent-persistence.md)
