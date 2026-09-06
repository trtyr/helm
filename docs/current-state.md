# Current State — 已验证基线

> 本文件记录**当前**实测的门禁结果与开放项，随落地同步更新。

## 验证结果（2026-09-06 实测，Phase 12 单用户账号管理落地）

| 命令 | 结果 |
|------|------|
| `cargo fmt --all -- --check` | ✅ exit 0（格式通过） |
| `cargo clippy --all-targets -- -D warnings` | ✅ "No issues found"（零警告） |
| `cargo test` | ✅ 全绿 80 测试（新增 auth_account_test 3 集成：/me 数据源/改密/改名） |
| `python scripts/check_openapi.py` | ✅ exit 0（51 端点，与 server 路由一致） |
| `python scripts/check_docs.py` | ✅ exit 0（9 迁移，文档与代码一致） |
| `python scripts/e2e-skill.py` | ✅ 五段全过（key 下载 zip/清单 sha256/解包即用/401/吊销闭环） |
| `python scripts/e2e-account.py` | ✅ 六段全过（/me/改密全链路/改密后旧密 401/改名/改名后旧 token 失效/凭据还原） |
| `console`: tsc -b + vite build + oxlint + vitest | ✅ 构建/48 测试全过（Node 在 `C:\Program Files\nodejs`，不在本机 PATH，需手动加） |
| `node scripts/verify-m5-account.mjs`（playwright + 真后端） | ✅ 六项全过（账号卡渲染/改密错密行内提示/改密成功强制重登+notice/新密登录/凭据还原） |
| Postgres（`helm-postgres`） | ✅ Up，healthy |
| API key 全链路 e2e | ✅ 九项全过（登录→建 key→key 调 API→自管 403→列表仅前缀→错 key 401→吊销即 401+幂等→过期 401→审计 actor） |

测试组成（73 = 单元测试 + 集成测试；Phase 10 新增：`api_key_service` 纯函数 5 个 +
`api_key_test` 集成 4 个；另修复 Windows 无 symlink 权限时 `fs::list_dir_symlink_to_dir_is_dir`
的环境性失败、通知测试 `mark_all_read` 全局口径的并行竞态）：

- 单元测试覆盖：`job` 状态机、`auth` JWT、`file_service` checksum、`forward_service` 非法地址、
  `scheduler` 参数解析、`agent_service` token/status/`map_service_status`、`connection_registry`、
  `transfer_registry`、`session_registry`、`stream_registry`（subscribe/broadcast/清理）、
  `online_status`（is_stale）、`cert_service`（CA/CSR/证书 + `load_or_generate` 重启稳定 +
  `issue_agent_cert` 三件套）、`alert_service`（threshold_for）、`forward_manager`（diff_hosts
  差分启停/addr 变更重启）、`encoding`（GBK 解码/ASCII 不变/未知页回退/非法字节替换符）、
  `notification_service`（within_cooldown 窗口边界/时钟回拨、should_notify_stale 只补发刚进入
  stale 的）、`api_key_service`（明文格式/唯一性/哈希稳定/展示前缀/合成 Claims）等。
- 集成测试（连真实 Postgres）：`store`（host 增删改分页、audit/alert 落库清理）、`exec_service`、
  `listener`（create/start/stop + 自清理）、`agent_lifecycle`（注销 + 孤儿主机软删）、
  `notification`（类型独立/同类型冷却合并 + read 重置/unread 过滤与已读/超阈值指标预警联动）、
  `api_key`（创建校验 roundtrip/吊销即失效+幂等/过期拒绝/列表+last_used_at 刷新）。

注意：`buf lint` / `buf breaking` 本机未装 buf 未跑（proto 本次无变更）；CI 与跨平台
symlink 语义见开放项。

真机实证（Linux 公网 forward 模式，含 mTLS）见
[real-machine-test-report.md](real-machine-test-report.md)：15/15 项通过，Windows GBK 乱码已修复验证。

## Git 状态

- 分支：`master`，跟踪 `origin/master`（github.com/trtyr/helm，私有）。
- Phase 9（`15005d8`）之后：前端 M1–M4 并仓（`ff3a805`）、Phase 10 API key
  （决策 010：`api_keys` 表 + Bearer 前缀分流 + api-keys 管理端点 + WS/HTTP 统一凭据校验）、
  Phase 11 Skill 包分发（决策 011：仓库 `skill/` 内嵌 Server，`/skill` 下载 + `/skill/manifest` 清单）、
  Phase 12 单用户账号管理（/auth/me + 改密 + 改名，Settings 账号卡 + Login notice，verify-m5-account.mjs）、
  proto 构建自带 vendored protoc（无系统 protobuf 也可编译）。

## 开放项 / 已知问题

1. **前端控制台同仓**：`console/`（Vite + React + TS strict，单仓），M1–M4 里程碑全落地——
   20 路由全部就绪（主机/详情五 tab/终端/文件/任务/通知/告警/审计/监听器/设置/仪表盘/forward），
   侧栏无「后续」灰化项；各里程碑以 console/scripts/verify-m1~m4.mjs 真后端联调验收
   （详见 plantree frontend 落地口径）。设计规格双链 docs/plantree/plans/frontend/
   （feature-inventory 70 功能已逐条标注落地轮次）。前端尚未接入 API key 登录（控制台仍走密码 JWT）。
2. **无 CI 配置**：没有 `.github/workflows`，门禁全靠本地 `just check` + `buf` + `check_openapi.py` + `check_docs.py`。
3. **无 Agent 交叉编译自动化脚本**：交叉编译命令已文档化（见 [run-and-deploy.md](run-and-deploy.md)），但无一键脚本或 CI 产物（本地仅 macOS 目标；真机测试用 musl 手动交叉编译）。
4. **开发默认凭据**：`admin/admin123`、`dev-token-change-me`、`dev-secret-change-me` 均为明文默认值，生产必须覆盖（代码注释已标注）。
5. **RBAC 未强制**：`users.role` 已存储（admin/operator）但 HTTP 层未按角色授权，单用户场景暂缓（见 plantree 决策 007）；API key 同为全权凭据（无 per-key 作用域，见决策 010 代价段）。
6. **预警阈值硬编码**：`AlertService::threshold_for`（cpu/mem/disk > 90%）为常量表，无 API 可调；通知中心已落地（Phase 9，决策 009——通知为系统内小卡片，不做外发），预警联动通知同走此阈值。
7. **exec 解码为整段后处理**：Windows OEM 解码在命令结束后整段进行（无跨 chunk 边界问题）；PTY 链路 ConPTY 输出 UTF-8 不受影响，但极端场景（非 ConPTY 的老 Windows PTY）未验证。
8. **Windows symlink 测试按权限跳过**：`fs::list_dir_symlink_to_dir_is_dir` 在无管理员/开发者模式的
   Windows 上自动跳过（建不了 symlink），CI 如在 Linux 跑则全量覆盖。
9. **buf 未装时 buf 门禁跳过**：`just buf-lint` 依赖本机 buf 二进制（本次验证环境未装，proto 无变更故未跑）。
10. **改密不改签 JWT**：change-password 后已有 JWT 仍有效至自然过期（24h）；改名则旧 token 的
    sub 立即失效（/auth/me 等按 sub 查库的端点返回 401）。全局撤销 = 换 `HELM_JWT_SECRET` 重启。
11. **本机 Node 不在 PATH**：Node v24 装在 `C:\Program Files\nodejs` 但未加入 PATH，
    console 门禁需手动 `export PATH="/c/Program Files/nodejs:$PATH"`；console/scripts/verify-m4.mjs
    有一处 oxlint warning（TOKEN 未使用，遗留）；`api:gen` 相对路径已在单仓布局下修正。
