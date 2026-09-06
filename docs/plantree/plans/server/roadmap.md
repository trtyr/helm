# Roadmap — Server

Role: current-state
Status: active

阶段划分遵循「底座先行」：Phase 0/1 是底座，Phase 2+ 是功能；Phase 5+ 是运维能力扩展（**参考 C2 控制面，不做隐蔽性设计**）。底座未稳不进入功能期。

## Done（已落地，有可复现证据）

- **Phase 0 — 工程骨架与底座** ✅
  - Cargo workspace：`server` + `agent` + `proto` 共享 crate
  - protobuf 契约：`agent/v1/agent.proto`、`types.proto`（双向流服务定义）
  - 分层目录 + 模块边界（domain/application/grpc/http/store）
  - 配置加载（clap）+ 可观测性（tracing + `/healthz`）
  - 存储底座：sqlx + Postgres migrations + 实体 schema
  - 门禁：fmt + clippy + test + `buf` 契约检查
- **Phase 1 — 通信底座 + 最小链路** ✅
  - gRPC server 双向流 + connection_registry
  - Agent 反向连接：注册 + token 认证 + 心跳
  - 命令执行链路：下发命令 → 流式拿回输出
  - 断线重连
- **Phase 2 — HTTP API + 存储落地** ✅
  - axum HTTP API：hosts / tasks / jobs / files / metrics
  - sqlx 仓储落地，Job 状态机落库
  - 控制台认证（JWT）
- **Phase 3 — 功能补全** ✅
  - 文件传输（分块 + 进度 + 校验和）
  - 状态监控（指标采集 + 落库）
  - 脚本/任务下发（定时调度 + 重启恢复）
- **Phase 4 — 正向连接** ✅
  - Agent 正向监听模式（Server 主动连）
  - `POST /api/v1/forward/exec` + 按主机名拨号
  - 已端到端验证（Linux/Windows 正向 exec 均通过）

## Done（续：Phase 5–8 运维能力扩展）

- **Phase 5 — 控制平面：监听器 + 在线状态 + Agent 生命周期** ✅
  - 监听器实体（id/addr/proto/auth）+ API 管理（create/list/start/stop）+ 持久化 + 重启恢复
  - 在线/离线判定（心跳超时 `is_stale`）+ API 暴露（`hosts` 带 online/last_seen/stale）
  - Agent 生命周期：注销（DELETE /agents/{id}）+ 自杀 / 卸载（SelfDestruct 清二进制 + 自启）
  - 落地证据：commit `c8c5cbc`；e2e-phase5.py；listener / online-status / agent-lifecycle 测试
- **Phase 6 — 远程管理：会话 + 服务管理 + 文件 + 进程** ✅
  - 交互会话 / 终端（WebSocket + portable-pty PTY，多开 + 空闲超时）
  - 常驻服务 / 后台任务管理（ServiceManager + restart_policy）
  - 文件管理（目录浏览 / 批量 / 上传下载）
  - 进程管理（list / kill）、网络信息采集
  - Agent 分组 / 标签管理（hosts.tags 过滤）
  - 落地证据：commit `6fac311` / `230ba7c` / `8ba3f09`；e2e-phase6.py 7 段
- **Phase 7 — 平台化：服务化 + 平台优化 + 安全 + 监控** ✅
  - Agent 服务化：Windows 去黑窗口（windows_subsystem）+ Linux systemd + deploy 模板
  - 安全：mTLS（rcgen 内置 CA + 自动签发）+ 审计日志（单用户，暂不 RBAC）
  - 监控扩展：磁盘 / 网络 / 进程指标 + 告警 alerts 表 + 时序保留（30 天）
  - 落地证据：commit `c396462`；e2e-phase7.py；44 测试
- **Phase 8 — API 完整性 + 前端契约** ✅
  - API 全量：DELETE / UPDATE、分页 / 过滤、WebSocket 实时流（服务日志 / job 输出 / metrics）、agents 详情
  - 前端控制台接口契约：openapi.yaml（OpenAPI 3.0.3）+ check_openapi.py 机器校验
  - 落地证据：commit `a3e0300`；e2e-phase8.py；47 测试
- **Phase 9 — 通知中心（系统内通知）** ✅
  - 定义（[决策 009](decisions/009-in-app-notifications.md)）：通知 = 系统内部小卡片消息——
    主机上线 / 下线 / 预警；**不做外发**（webhook / 邮件 / 钉钉）。
  - `notifications` 表（迁移 0008：type=online/offline/alert + 已读/未读）；`alerts` 保留预警时序历史，
    预警落库时联动生成通知。
  - 上/下线事件埋点：注册即发 online（reverse/forward 两路）、`on_disconnect` 断连即发 offline、
    心跳超时兜底扫描（`spawn_offline_sweeper`，含半开连接清理）。
  - 冷却窗口：同 host 同类型 5 分钟内合并为一条（刷新消息与时间、重置未读）。
  - API：`GET /notifications`（分页 + unread 过滤）/ `unread-count` / `{id}/read` / `read-all`
    + WS `/notifications/stream`（复用 StreamRegistry）。
  - 落地证据：e2e-phase9.py 五段全过（上线/冷却合并/已读未读/WS 推送/预警联动 gate）；
    63 测试（含 `within_cooldown`/`should_notify_stale` 纯函数 + 4 个通知集成测试）；
    openapi 44 端点校验通过。
- **Phase 10 — API key（机器对机器认证）** ✅
  - 定义（[决策 010](decisions/010-api-keys.md)）：`helm_` 前缀长效密钥，
    `api_keys` 表仅存 sha256 哈希 + 展示前缀，明文仅创建响应返回一次。
  - 生命周期：expires_at / revoked_at（幂等吊销）/ last_used_at（认证命中刷新）；
    已吊销 key 随保留清理 30 天后删除。
  - 认证接入：`verify_bearer_token` 统一 HTTP 中间件与 WS query-param 端点的凭据校验，
    Bearer 按 `helm_` 前缀分流 JWT / API key；命中合成 Claims（`api-key:<name>`）供审计。
  - API：`GET/POST /api-keys` + `GET/DELETE /api-keys/{id}`（仅 JWT，key 不可自管）。
  - 落地证据：api_key_test.rs 4 个集成测试（roundtrip/吊销/过期/列表+刷新）+ service 5 个单测；
    openapi 46 端点校验通过；e2e 冒烟（登录→建 key→key 调 API→吊销→401）见 current-state。
- **Phase 11 — Skill 包分发（决策 011）** ✅
  - 包源收拢仓库 `skill/`（SKILL.md + 16 个 Python 分域脚本 + 3 篇 references），
    `include_dir` 编译期内嵌进 Server 二进制。
  - `GET /api/v1/skill`（zip，脚本带可执行位）+ `GET /api/v1/skill/manifest`
    （版本 + 文件 sha256，确定性生成）；JWT / API key 均可（key 是取用凭据，防死锁）。
  - skill_package 4 个单测：布局齐全 / zip 可解压且逐文件一致 / manifest sha256 / 构建确定性；
    e2e-skill.py 沉淀（key 下载→解包→包内脚本真操作平台→无凭据 401）。
  - openapi 48 端点校验通过。
- **Phase 12 — 单用户账号管理** ✅
  - `GET /auth/me`（按 JWT sub 查库取权威身份 `{username, role, created_at}`；sub 失效 → 401 强制重登）。
  - `POST /auth/change-password`（校验当前密码 + 新密码 ≥6 字符；已有 JWT 不失效，24h 自然过期）。
  - `POST /auth/change-username`（校验当前密码 + UNIQUE 查重含软删行 23505→400；旧 sub 随即失效）。
  - 三端点仅 JWT（API key 403——key 无账号概念）；改密/改名记审计 `password_change`/`username_change`。
  - 前端 Settings 新增「账号」卡（身份展示 + 改名/改密表单，成功后清 token 回登录页带 notice）；
    Login 页支持 `?notice=` 提示条。
  - auth_account_test.rs 3 个集成测试；openapi 51 端点校验通过。

## Next（已规划，未开工）

（暂无——Phase 12 已落地，见 Done。）

## Deferred

- 多租户
- RBAC / 权限体系
- 会话录像 / 审计回放
- Agent 插件机制
- 高可用 / 集群部署
- 配置管理编排（Ansible 类 playbook）

## 落地证据约定

每个 Phase 的「完成」须有可复现证据（见 [baseline 门禁](../../baseline/test-and-release-gates.md)）：
构建通过、测试通过、契约检查通过、以及该 Phase 的链路 smoke 结果。
当前落地证据：`git log`（`c8c5cbc` Phase 5、`6fac311`+`230ba7c`+`8ba3f09` Phase 6、`c396462` Phase 7、`a3e0300` Phase 8），
以及 [docs/current-state.md](../../../current-state.md) 的实测门禁结果（fmt/clippy/test 47/buf/check_openapi 全绿）。
