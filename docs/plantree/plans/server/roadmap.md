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
