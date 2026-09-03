# Current State — 已验证基线

> 本文件记录**当前**实测的门禁结果与开放项，随落地同步更新。

## 验证结果（2026-09-03 实测，HEAD `3ffb691`）

| 命令 | 结果 |
|------|------|
| `cargo fmt --all -- --check` | ✅ exit 0（格式通过） |
| `cargo clippy --all-targets -- -D warnings` | ✅ "No issues found"（零警告） |
| `cargo test` | ✅ 56 passed（10 suites） |
| `buf lint` | ✅ exit 0 |
| `buf breaking --against '.git#ref=HEAD~1'` | ✅ exit 0 |
| `python3 scripts/check_openapi.py` | ✅ exit 0（39 端点，与 server 路由一致） |
| `python3 scripts/check_docs.py` | ✅ exit 0（39 文档扫描，无旧痕迹） |
| `python3 scripts/e2e-smoke.py` | ✅ smoke OK（Postgres + Server + Agent 全链路） |
| Postgres（`helm-postgres`） | ✅ Up，healthy |

测试组成（56 = 单元测试 + 集成测试；Phase 8 时 47，其后 +4 `diff_hosts`、
+4 `decode_with_codepage`、+1 cert 持久化）：

- 单元测试覆盖：`job` 状态机、`auth` JWT、`file_service` checksum、`forward_service` 非法地址、
  `scheduler` 参数解析、`agent_service` token/status/`map_service_status`、`connection_registry`、
  `transfer_registry`、`session_registry`、`stream_registry`（subscribe/broadcast/清理）、
  `online_status`（is_stale）、`cert_service`（CA/CSR/证书 + `load_or_generate` 重启稳定 +
  `issue_agent_cert` 三件套）、`alert_service`（threshold_for）、`forward_manager`（diff_hosts
  差分启停/addr 变更重启）、`encoding`（GBK 解码/ASCII 不变/未知页回退/非法字节替换符）等。
- 集成测试（连真实 Postgres）：`store`（host 增删改分页、audit/alert 落库清理）、`exec_service`、
  `listener`（create/start/stop + 自清理）、`agent_lifecycle`（注销 + 孤儿主机软删）。

真机实证（Linux 公网 forward 模式，含 mTLS）见
[real-machine-test-report.md](real-machine-test-report.md)：15/15 项通过，Windows GBK 乱码已修复验证。

## Git 状态

- 分支：`master`，工作区干净；**无 remote**。
- 最近落地（Phase 8 `a3e0300` 之后）：
  `66aa763` forward 持久连接（ForwardManager + InboundCtx 抽取，全端点可用）、
  `06871e3` Windows GBK 控制台解码（encoding.rs）、
  `b9eef82` forward 模式 mTLS（CA 持久化 `--tls-dir` + `--issue-cert` 离线签发 + 双向 TLS 拨号）、
  `c50a1b1` Linux 真机测试报告、`3ffb691` run-and-deploy 补 mTLS 预置流程。

## 开放项 / 已知问题

1. **无前端**：控制台是独立工程，本仓库只有后端 + HTTP API + OpenAPI 契约（见 [api.md](api.md) / [openapi.yaml](openapi.yaml)）。
2. **无 CI 配置**：没有 `.github/workflows`，门禁全靠本地 `just check` + `buf` + `check_openapi.py` + `check_docs.py`。
3. **无 Agent 交叉编译自动化脚本**：交叉编译命令已文档化（见 [run-and-deploy.md](run-and-deploy.md)），但无一键脚本或 CI 产物（本地仅 macOS 目标；真机测试用 musl 手动交叉编译）。
4. **开发默认凭据**：`admin/admin123`、`dev-token-change-me`、`dev-secret-change-me` 均为明文默认值，生产必须覆盖（代码注释已标注）。
5. **RBAC 未强制**：`users.role` 已存储（admin/operator）但 HTTP 层未按角色授权，单用户场景暂缓（见 plantree 决策 007）。
6. **告警无外发通道**：告警只落 `alerts` 表 + `GET /api/v1/alerts` 查询，无 webhook/邮件/钉钉通知（Phase 7 范围外）。
7. **exec 解码为整段后处理**：Windows OEM 解码在命令结束后整段进行（无跨 chunk 边界问题）；PTY 链路 ConPTY 输出 UTF-8 不受影响，但极端场景（非 ConPTY 的老 Windows PTY）未验证。
