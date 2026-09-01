# Current State — 已验证基线

> 本文件由 project-init 的 Verify 阶段实测生成，记录**当前**命令输出与开放项。

## 验证结果（2026-09 实测）

| 命令 | 结果 |
|------|------|
| `cargo fmt --all -- --check` | ✅ exit 0（格式通过） |
| `cargo clippy --all-targets -- -D warnings` | ✅ "No issues found"（零警告） |
| `cargo test` | ✅ 16 passed（8 suites） |
| `buf lint` | ✅ exit 0 |
| Postgres（`helm-postgres`） | ✅ Up，healthy |

测试组成（16 = 14 单元 + 2 集成）：

- 单元：`job` 状态机 ×2、`auth` JWT ×2、`file_service` checksum ×2、`forward_service` 非法地址 ×1、`scheduler` 参数解析 ×2、`agent_service` token/status ×2、`connection_registry` ×1、`transfer_registry` ×2。
- 集成（连真实 Postgres）：`exec_service` 未知 agent → NotFound ×1、`store` host 增删 ×1。

## Git 状态

- 分支：`master`；**无 remote**。
- 最近提交（7 个）：从 `feat: 集中式运维平台后端（Server + Agent + gRPC 双向流底座）` 到 `feat: 正向代理主机创建 API + 按主机名正向连接`。
- **未提交改动**：
  - 代码（3 文件）：`agent/src/forward.rs`（+3 行 tracing 日志）、`server/src/application/forward_service.rs`（正向拨号自动补 `http://`）、`.pi/.goals-pool-snapshot.json`（goal 快照）。
  - 本次「修复」改动：`git rm src/main.rs`（已 staged）+ 根 `README.md` + `docs/*.md` 与 `docs/plantree/**` 的文档刷新。

## 开放项 / 已知问题

1. ~~孤儿 `src/main.rs`~~ ✅ **已修复**：`git rm` 删除，workspace 编译不受影响。
2. ~~`docs/plantree/` 过时~~ ✅ **已修复**：baseline / roadmap / 决策索引 / topics / open-questions 已刷到现状（决策索引补 004，Phase 0–3 标注完成，Phase 4 部分完成）。
3. ~~根 README 环境变量表漂移~~ ✅ **已修复**：配置表拆为 Server/Agent 两表，`HELM_SERVER_TOKEN` 默认更正为 `dev-token-change-me` 并注明空则拒绝。
4. **无前端**：控制台是独立工程，本仓库只有后端 + 契约；HTTP API 是未来前端要消费的接口（见 [api.md](api.md)）。
5. **无 CI 配置**：没有 `.github/workflows`，门禁全靠本地 `just check` + `buf`。
6. **无 Agent 交叉编译脚本**：README 声称跨平台单二进制，但仓库内无 Windows/Linux 交叉编译的脚本或 CI 产物（本地仅 macOS 目标）。
7. **开发默认凭据**：`admin/admin123`、`dev-token-change-me`、`dev-secret-change-me` 均为明文默认值，生产必须覆盖（代码注释已标注）。
