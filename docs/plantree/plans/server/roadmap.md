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

## Next（运维能力扩展，规划中）

- **Phase 5 — 控制平面：监听器 + 在线状态 + Agent 生命周期**
  - 监听器实体（id/addr/proto/auth）+ API 管理（create/list/start/stop）+ 持久化
  - 在线/离线判定（心跳超时）+ API 暴露（`hosts` 带 online/last_seen）+ 状态变更事件
  - Agent 生命周期：注册 / 下线 / 注销 + **自杀 / 卸载机制**（Agent 自我清除）
  - 完成标准：监听器可经 API 启停；`hosts` 返回真实在线状态；agent 可下线 / 自卸载；离线判定有 e2e 验证

- **Phase 6 — 远程管理：会话 + 服务管理 + 文件 + 进程**
  - **交互会话 / 终端**（WebSocket 实时双向流 + PTY，SSH 终端那类）
  - **持久化服务 / 后台任务管理**（部署常驻任务 + 持续监听状态 / 日志 / 重启，systemd / supervisor 那类）
  - 文件管理完善（目录浏览 / 批量 / 上传下载，文件管理器）
  - 进程管理（list / kill）、网络信息采集
  - Agent 分组 / 标签管理
  - 完成标准：会话 / 服务 / 文件 / 进程各有 API + e2e smoke

- **Phase 7 — 平台化：服务化 + 平台优化 + 安全 + 监控**
  - Agent 服务化：Windows 服务（去黑窗口）+ Linux systemd + 自启动
  - Linux / Windows 平台优化（进程 / 文件 / 服务管理按平台适配）
  - 安全：mTLS + 审计日志（**单用户，暂不 RBAC**）
  - 监控扩展：磁盘 / 网络 / 进程列表 + 告警 + 时序保留策略
  - 完成标准：Agent 服务化并自启；mTLS 握手成功；审计有单测

- **Phase 8 — API 完整性 + 前端契约**
  - API 全量：DELETE / UPDATE、分页 / 过滤、WebSocket 实时流（会话 / 日志）、agents 详情
  - 前端控制台接口契约（独立工程，本树只定义契约）
  - 完成标准：API 覆盖完整 CRUD；实时流可用；契约文档落地

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
当前落地证据：`git log`（`2b97ae8` 底座 + 通信链路，`c0456f0` 正向连接 API），
以及 [docs/current-state.md](../../../current-state.md) 的实测门禁结果（fmt/clippy/test/buf 全绿）。
