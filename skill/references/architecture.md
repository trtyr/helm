# Helm 平台架构与认证模型

## 是什么

集中式运维平台：Server 控制端 + 跨平台 Agent 被控端 + React 控制台。
Agent 装在目标机（Windows/Linux/macOS），控制端可对整批主机做远程命令执行、
文件上下传、交互终端、常驻服务管理、进程/网络管控、指标采集告警、审计。

```
[控制台/脚本] ──HTTP/WS──► Server（axum :8080）◄──gRPC 双向流(可选 mTLS)──► Agent（各目标机）
                                │
                                └── Postgres（hosts/agents/jobs/metrics/alerts/notifications/...）
```

## 连接模式

- **reverse（默认）**：Agent 主动拨 Server 的 gRPC（:50051），穿 NAT。
- **forward**：Agent 监听 gRPC 端口，Server 拨号（同网段/公网只监听不回连）。
  - 持久连接：Server 侧 reconciler 差分维护，注册后全端点可用；
  - 按需：`POST /forward/exec` 单次拨号执行。

两种模式同一套 protobuf 信令（`OpenChannel` / `OpenForwardChannel` 双向流）。

## 认证模型（决策 010）

| 凭据 | 形态 | 用途 |
|------|------|------|
| JWT | `POST /auth/login` 换取，HS256，24h | 控制台用户；**api-keys 管理端点只收 JWT** |
| API key | `helm_` + 40 hex，长效 | 机器对机器（CI、脚本、本 skill）；除 api-keys 管理外与 JWT 等效 |
| Agent token | `HELM_SERVER_TOKEN` 严格匹配 | Agent gRPC 注册（非 HTTP API） |

- 服务端按 Bearer 值前缀分流：`helm_` 开头 → sha256 比对 `api_keys` 表（仅存哈希）；
  否则按 JWT 解码。WS `?token=` 同一函数。
- API key 命中后审计 actor 记 `api-key:<name>`；`last_used_at` 认证时刷新。
- key 生命周期：`expires_at`（可空）、`revoked_at`（吊销幂等）；已吊销 30 天后清理。
- mTLS：反向模式 token 换证书（`POST /agents/cert`）；正向模式管理员离线签发（`--issue-cert`）。

## 数据模型要点

- `hosts`（目标机，软删）↔ `agents`（注册实例，agent_id 为业务主键）
- `jobs`（命令执行实例，状态机 queued→running→succeeded|failed|timed_out|cancelled）
- `metrics`（时序，30 天保留）、`alerts`（阈值告警）、`notifications`（在线/离线/预警，5min 冷却合并）
- `listeners`（gRPC 监听器为 DB 实体，可动态启停、重启恢复）
- `services`（目标机常驻任务）、`audit_logs`（actor/action/resource/detail）
- `api_keys`（哈希、前缀、过期/吊销时间戳）

## Server 配置（环境变量）

`HELM_HTTP_ADDR`(:8080)、`HELM_GRPC_ADDR`(:50051)、`HELM_DATABASE_URL`、
`HELM_SERVER_TOKEN`（Agent 注册）、`HELM_JWT_SECRET`、`HELM_HEARTBEAT_TIMEOUT`(30s)、
`HELM_SESSION_IDLE_TIMEOUT`(300s)、`HELM_MTLS`/`HELM_TLS_DIR`/`HELM_TLS_SERVER_NAME`。

本地起平台（仓库内）：

```bash
docker compose up -d postgres
cargo run -p helm-server            # 首启自动迁移 + seed admin/admin123
cargo run -p helm-agent -- --agent-id my-host --token dev-token-change-me
```
