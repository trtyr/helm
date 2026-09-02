# Current State — 已验证基线

> 本文件记录**当前**实测的门禁结果与开放项，随落地同步更新。

## 验证结果（2026-09 实测）

| 命令 | 结果 |
|------|------|
| `cargo fmt --all -- --check` | ✅ exit 0（格式通过） |
| `cargo clippy --all-targets -- -D warnings` | ✅ "No issues found"（零警告） |
| `cargo test` | ✅ 47 passed（10 suites） |
| `buf lint` | ✅ exit 0 |
| `buf breaking --against '.git#ref=HEAD~1'` | ✅ exit 0 |
| `python3 scripts/check_openapi.py` | ✅ exit 0（39 端点，与 server 路由一致） |
| `python3 scripts/e2e-phase8.py` | ✅ exit 0（CRUD + 三个实时流） |
| Postgres（`helm-postgres`） | ✅ Up，healthy |

测试组成（47 = 单元测试 + 集成测试）：

- 单元测试覆盖：`job` 状态机、`auth` JWT、`file_service` checksum、`forward_service` 非法地址、
  `scheduler` 参数解析、`agent_service` token/status/`map_service_status`、`connection_registry`、
  `transfer_registry`、`session_registry`、`stream_registry`（subscribe/broadcast/清理）、
  `online_status`（is_stale）、`cert_service`（CA/CSR/证书）、`alert_service`（threshold_for）等。
- 集成测试（连真实 Postgres）：`store`（host 增删改分页、audit/alert 落库清理）、`exec_service`、
  `listener`（create/start/stop + 自清理）、`agent_lifecycle`（注销 + 孤儿主机软删）。

## Git 状态

- 分支：`master`；**无 remote**。
- 最近提交（Phase 5–8 落地）：`a3e0300` Phase 8、`c396462` Phase 7、`6fac311` + `230ba7c` + `8ba3f09` Phase 6、`c8c5cbc` Phase 5。

## 开放项 / 已知问题

1. **无前端**：控制台是独立工程，本仓库只有后端 + HTTP API + OpenAPI 契约（见 [api.md](api.md) / [openapi.yaml](openapi.yaml)）。
2. **无 CI 配置**：没有 `.github/workflows`，门禁全靠本地 `just check` + `buf` + `check_openapi.py`。
3. **无 Agent 交叉编译自动化脚本**：交叉编译命令已文档化（见 [run-and-deploy.md](run-and-deploy.md)），但无一键脚本或 CI 产物（本地仅 macOS 目标）。
4. **开发默认凭据**：`admin/admin123`、`dev-token-change-me`、`dev-secret-change-me` 均为明文默认值，生产必须覆盖（代码注释已标注）。
5. **RBAC 未强制**：`users.role` 已存储（admin/operator）但 HTTP 层未按角色授权，单用户场景暂缓（见 plantree 决策 007）。
6. **告警无外发通道**：告警只落 `alerts` 表 + `GET /api/v1/alerts` 查询，无 webhook/邮件/钉钉通知（Phase 7 范围外）。
