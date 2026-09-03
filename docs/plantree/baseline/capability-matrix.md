# 运维能力现状矩阵

Role: detail-shard（baseline 全局上下文）
Status: active

运维平台的能力现状评估（**参考 C2 控制面能力，不做隐蔽性 / 规避设计**）。对照「连接模式 / 监听器 / 在线离线 / 控制 / 监控 / 持久化 / 安全 / API 完整性 / 通知」九个能力域，标注已实现与缺失。每条引用真实文件或 API 佐证，不虚构能力。以当前 `HEAD`（`3ffb691`，Phase 8 已落地）为基线。

## 能力域总览

| 能力域 | 状态 | 一句话 |
|--------|------|--------|
| 连接模式 | ✅ 齐 | 反向（Agent 主动连）+ 正向（Server 主动连）双模式同构 |
| 监听器 | ✅ 齐 | DB 实体 + API 启停 + 多实例 + 重启恢复 |
| 在线/离线状态 | ✅ 齐 | 心跳超时判定 + `hosts` 暴露 online/last_seen/stale |
| 控制能力 | ✅ 齐 | 命令/文件/脚本/定时 + 交互 shell/进程/网络/文件浏览/分组 |
| 监控 | ✅ 齐 | cpu/mem/disk/net/proc 指标 + 阈值预警 + 时序保留 |
| 持久化 | ✅ 齐 | Windows 去黑窗口 + 服务化 + Linux systemd |
| 安全 | ✅ 齐 | token+JWT+bcrypt+mTLS+审计（RBAC 暂缓，单用户） |
| API 完整性 | ✅ 齐 | 全 CRUD + 分页/过滤 + 实时流 + OpenAPI 契约 |
| 通知 | ✅ 齐 | 系统内小卡片（上线/下线/预警 + 已读未读 + WS 推送 + 5 分钟冷却合并），Phase 9 已落地（[决策 009](../plans/server/decisions/009-in-app-notifications.md)） |

## 各能力域明细

### 1. 连接模式 ✅

- **反向（默认）**：Agent 主动连 Server。`agent/src/connection.rs` 拨号 → `server/src/grpc/agent_service.rs` 的 `AgentService.OpenChannel` 双向流。注册、token 认证、心跳（10s）、命令、文件、指标、会话、服务、进程、网络。
- **正向**：Server 主动拨 Agent。`agent/src/forward.rs` 的 `ForwardAgentService.OpenForwardChannel` + `POST /api/v1/forward/exec`，端到端验证通过。
- 缺口：正/反向无统一「会话」概念（各有独立实现，信令同构）。

### 2. 监听器 ✅

- `listeners` 表（migration 0004）+ `ListenerRegistry`（动态启停，oneshot 关停）+ `ListenerService`（create/list/start/stop + resume_or_seed 启动恢复）。
- HTTP：`GET|POST /api/v1/listeners`、`PUT|DELETE /api/v1/listeners/{id}`、`POST /api/v1/listeners/{id}/start|stop`。
- 支持多实例 + 重启恢复；默认监听器 seed（空表时）。

### 3. 在线/离线状态 ✅

- 心跳：`agent/src/connection.rs` 每 10s 发 `Heartbeat` → `AgentRepo.update_heartbeat` 写 `agents.last_heartbeat_at`。
- 在线表：`ConnectionRegistry`（内存 HashMap，`any_online` 聚合主机级在线）。
- 超时判定：`application/online_status.rs::is_stale(last_seen, now, timeout)`（阈值可配，默认 30s）。
- API：`GET /api/v1/hosts` 返回 online / last_seen / stale。

### 4. 控制能力 ✅

- 命令执行 `POST /api/v1/exec`；文件上下传/列目录 `POST /api/v1/files/upload|download|list`（sha256 校验）；脚本/定时 `POST /api/v1/tasks/script|schedule`。
- 交互 shell：`GET /api/v1/agents/{id}/terminal`（WebSocket + portable-pty PTY，多开 + 空闲超时）。
- 进程/网络：`POST /api/v1/processes/list|kill`、`POST /api/v1/net/info`。
- 分组/标签：`POST /api/v1/hosts/{id}/tags` + `GET /api/v1/hosts?tag=` 过滤。
- Agent 下线/卸载：`DELETE /api/v1/agents/{id}` + `POST /api/v1/agents/{id}/uninstall`（SelfDestruct）。

### 5. 监控 ✅

- `agent/src/monitor.rs` 每 30s 采集 `cpu.usage` / `mem.*` / `disk.usage` / `net.rx|tx_bytes` / `proc.count`，经 `MetricReport` 上报落库。
- 阈值预警：`AlertService::threshold_for`（cpu/mem/disk > 90%）→ 落 `alerts` 表 → `GET /api/v1/alerts`（时序历史语义，保留 30 天）。
- 时序保留：后台任务每 24h 清理 30 天前 `metrics` / `alerts`。
- 缺口：预警与上/下线事件均无「通知」形态——控制台使用者感知不到状态变化（见 §9 通知）。

### 6. 持久化 ✅

- Windows：`#![windows_subsystem = "windows"]` 去黑窗口 + `deploy/install-windows-service.ps1`（nssm 服务安装）。
- Linux：`deploy/helm-agent.service` systemd unit（Restart=always）。
- 日志落文件：`--log-dir`（tracing-appender 按天滚动）。

### 7. 安全 ✅

- Agent token 严格认证（`token_matches`）；控制台 JWT + bcrypt；单点错误边界（`safe_message`）。
- mTLS：rcgen 内置 CA + `POST /api/v1/agents/cert` 自动签发（`--mtls` / `--cert-dir`）。
- 审计日志：`audit_logs` 表 + `GET /api/v1/audit`（登录/exec/文件/主机/监听器落库）。
- 未做：RBAC 强制（`users.role` 存而不查，单用户暂缓）、命令白名单。

### 8. API 完整性 ✅

- 39 个端点（OpenAPI 3.0.3），全实体 CRUD（DELETE/UPDATE）+ 分页/过滤 + agents 详情。
- WebSocket 实时流：服务日志 tail-f / job 输出 / metrics。
- 契约：`docs/openapi.yaml` + `scripts/check_openapi.py` 机器校验。

### 9. 通知 ✅（Phase 9 已落地）

- **定义（[决策 009](../plans/server/decisions/009-in-app-notifications.md)）**：通知 = 系统内部小卡片消息，
  呈现给控制台使用者；内容三类——主机上线（online）/ 下线（offline）/ 预警（alert）；
  带已读/未读。**不做外发**（webhook/邮件/钉钉均不在范围）。
- 实现（Phase 9）：
  - `notifications` 表（migration 0008）+ `NotificationRepo` + `NotificationService`
    （`application/notification_service.rs`：冷却合并 + 落库 + StreamRegistry 广播）。
  - 事件埋点：注册即发 online（`agent_service.rs` / `forward_manager.rs` 两路同构）、
    断连即发 offline（`InboundCtx::on_disconnect`）、心跳超时兜底扫描
    （`spawn_offline_sweeper`：刚进入 stale 才补发 + 半开死连接注销）、预警联动（指标超阈值）。
  - 冷却窗口：同 host 同类型 5 分钟内合并为一条（刷新消息与时间、重置未读），压平抖动/重启风暴。
  - API：`GET /api/v1/notifications`（分页 + `unread` 过滤）/ `notifications/unread-count` /
    `POST /notifications/{id}/read` / `POST /notifications/read-all`
    + WS `/api/v1/notifications/stream`（实时推送）。
  - 时序保留：与 metrics/alerts 一致，30 天后台清理。
  - 验证：e2e-phase9.py 五段全过；63 测试全绿；openapi 44 端点一致。

## 参考

- 模块地图：[module-map.md](module-map.md)
- 运行时流程：[runtime-flows.md](runtime-flows.md)
- 存储与状态：[storage-and-state.md](storage-and-state.md)
- 风险热点：[risk-hotspots.md](risk-hotspots.md)
