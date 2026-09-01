# 运维能力现状矩阵

Role: detail-shard（baseline 全局上下文）
Status: active

运维平台的能力现状评估（**参考 C2 控制面能力，不做隐蔽性 / 规避设计**）。对照「连接模式 / 监听器 / 在线离线 / 控制 / 监控 / 持久化 / 安全 / API 完整性」八个能力域，标注已实现与缺失。每条引用真实文件或 API 佐证，不虚构能力。以当前 `HEAD`（正向连接 API 已落地）为基线。

## 能力域总览

| 能力域 | 状态 | 一句话 |
|--------|------|--------|
| 连接模式 | ✅ 基本齐 | 反向（Agent 主动连）+ 正向（Server 主动连）双模式同构 |
| 监听器 | ❌ 缺失 | gRPC 监听硬编码，无「监听器」概念 |
| 在线/离线状态 | ⚠️ 部分 | 有心跳 + 注册表，无超时判离线、无 API 暴露 |
| 控制能力 | ⚠️ 部分 | 命令/文件/脚本/定时有，无交互 shell、进程、网络 |
| 监控 | ⚠️ 部分 | cpu/mem/proc 有，无磁盘/网络/进程列表/告警 |
| 持久化 | ❌ 缺失 | 无服务化/开机自启（Windows 黑窗口） |
| 安全 | ⚠️ 部分 | token+JWT+bcrypt 有，无 mTLS / 审计（单用户，暂不 RBAC） |
| API 完整性 | ⚠️ 部分 | 基础读写有，无 DELETE/UPDATE / 在线状态 / 实时流 |

## 各能力域明细

### 1. 连接模式 ✅

- **反向（默认）**：Agent 主动连 Server。`agent/src/connection.rs` 拨号 → `server/src/grpc/agent_service.rs` 的 `AgentService.OpenChannel` 双向流。已实现：注册、token 认证、心跳（10s）、命令、文件、指标上报。
- **正向**：Server 主动拨 Agent。`agent/src/forward.rs` 的 `ForwardAgentService.OpenForwardChannel` + `server/src/application/forward_service.rs` + `POST /api/v1/forward/exec`。命令执行已端到端验证；文件信令（FileRequest/FileChunk 分支）逻辑存在但未端到端验证。
- 缺口：正向模式文件/指标信令未完整验证；正/反向无统一「会话」概念。

### 2. 监听器 ❌

- 现状：Server 的 gRPC 监听在 `server/src/grpc/mod.rs` 硬编码 `config.grpc_addr`，进程启动即监听，无生命周期管理。
- 缺失：无「监听器」实体（id / addr / proto / auth / 状态）、无 API 启停、无多监听器、无监听器配置持久化。

### 3. 在线/离线状态 ⚠️

- 现状：
  - 心跳：`agent/src/connection.rs` 每 10s 发 `Heartbeat`；`server/src/grpc/agent_service.rs` 收到后 `AgentRepo.update_heartbeat` 写 `agents.last_heartbeat_at`。
  - 在线表：`server/src/grpc/connection_registry.rs` 内存 HashMap（agent_id → 发送通道），`is_online` / `online_count`。
- 缺失：无心跳超时判离线（`last_heartbeat_at` 只存不查）；无 API 暴露在线状态（`GET /api/v1/hosts` 不含 online / last_seen）；无状态变更事件；无离线告警。

### 4. 控制能力 ⚠️

- 已实现：命令执行 `POST /api/v1/exec`；文件上下传 `POST /api/v1/files/upload|download`（sha256 校验）；脚本/定时 `POST /api/v1/tasks/script|schedule`（定时任务可重启恢复）。
- 缺失：交互 shell（实时双向流）；进程管理（list / kill）；网络信息采集；文件系统浏览；Agent 分组/标签管理（`hosts.tags` 存了但无过滤/管理端点）；Agent 下线/卸载。

### 5. 监控 ⚠️

- 已实现：`agent/src/monitor.rs` 每 30s 采集 `cpu.usage` / `mem.used` / `mem.total` / `mem.percent` / `proc.count`，经 `MetricReport` 上报落库。
- 缺失：磁盘 / 网络 / 进程列表；指标告警；时序保留策略（`metrics` 表无分区 / 滚动）。

### 6. 持久化 ❌

- 缺失：Agent 无开机自启 / 服务化。Windows 以控制台程序运行（有黑窗口）；Linux 无 systemd unit；无自升级。

### 7. 安全 ⚠️

- 已实现：Agent token 严格认证（`server/src/grpc/agent_service.rs` 的 `token_matches`）；控制台 JWT + bcrypt（`application/auth_service.rs`）；单点错误边界（`domain/error.rs` 的 `safe_message`，不外泄内部串）。
- 缺失：传输加密（明文 gRPC，无 TLS / mTLS）；审计日志；命令白名单 / 最小权限。权限模型当前单用户（`users.role` 字段保留但不校验，RBAC 后续再上）。

### 8. API 完整性 ⚠️

- 已实现：`POST /api/v1/auth/login`、`GET|POST /api/v1/hosts`、`POST /api/v1/exec`、`GET /api/v1/jobs/{id}`、`GET /api/v1/metrics`、`POST /api/v1/files/upload|download`、`POST /api/v1/tasks/script|schedule`、`POST /api/v1/forward/exec`、`GET /healthz`。
- 缺失：DELETE / UPDATE（`hosts` 只有 list/create，无删除/更新）；在线状态端点；WebSocket / 实时流（exec 结果只能轮询 `jobs/{id}`）；分页 / 过滤；agents 独立列表/详情；批量操作。

## 参考

- 模块地图：[module-map.md](module-map.md)
- 运行时流程：[runtime-flows.md](runtime-flows.md)
- 存储与状态：[storage-and-state.md](storage-and-state.md)
- 风险热点：[risk-hotspots.md](risk-hotspots.md)
