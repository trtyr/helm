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
| [009](009-in-app-notifications.md) | 通知为系统内部能力（小卡片），不做外发 | active | notifications |
| [010](010-api-keys.md) | API key：机器对机器认证（helm_ 前缀密钥，仅存哈希） | active | api-keys |
| [011](011-skill-package-serving.md) | Skill 包由 Server 内嵌分发（API key 即取即用） | active | skill-package |

## 主题索引

- communication-protocol → [001](001-grpc-as-communication-base.md)
- connection-model → [002](002-bidirectional-stream-connection-model.md)
- data-model → [004](004-storage-postgres-direct.md)（003 已取代）
- listener-management → [005](005-listener-model.md)
- online-status → [006](006-online-status-detection.md)
- security-hardening → [007](007-security-posture.md)
- agent-persistence → [008](008-agent-persistence.md)
- notifications → [009](009-in-app-notifications.md)
- api-keys → [010](010-api-keys.md)
- skill-package → [011](011-skill-package-serving.md)
