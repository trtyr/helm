# helm — 集中式运维平台

集中式运维平台后端：**Server 控制端** + 跨平台 **Agent 被控端**。
下发 Agent 到目标主机（Windows / Linux / macOS），即可对该主机进行远程运维：
命令执行、文件上下传、交互式终端、常驻服务管理、进程/网络管控、指标采集与告警、审计，
并提供完整 HTTP API 与 OpenAPI 契约供前端控制台消费。

## 架构

```text
[控制台 / HTTP API] ──► Server 控制端 ◄──gRPC 双向流（可选 mTLS）──► Agent 被控端（各目标主机）
```

- **Server**（`server/`）：集中管理、任务编排、状态收集、持久化（Postgres）、
  对外 HTTP API（含 WebSocket 实时流）。
- **Agent**（`agent/`）：装在目标机，执行命令、采集指标、文件传输、交互终端、
  常驻服务、进程/网络信息。单二进制，跨平台。
- **proto**（`proto/`）：gRPC 契约（protobuf），Server 与 Agent 的单一事实来源。

### 连接模式

- **反向**（默认）：Agent 主动连 Server，穿透 NAT/防火墙。
- **正向**：Agent 监听，Server 主动拨号（同区域内网）。

两种模式复用同一套信令协议（gRPC 双向流），见
[docs/plantree](docs/plantree/README.md) 中的架构决策。

## 目录结构

```text
Cargo.toml            # workspace（server / agent / proto）
proto/                # 共享 protobuf 契约（gRPC 底座）
server/               # Server 控制端（axum + tonic + sqlx）
  src/domain          #   领域层（实体、状态机）
  src/application     #   应用层（用例编排）
  src/grpc            #   gRPC 适配层（Agent 连入）
  src/http            #   HTTP API 适配层（控制台）
  src/store           #   持久化层（sqlx + Postgres）
  migrations/         #   数据库迁移（7 个版本）
agent/                # Agent 被控端（tokio，跨平台）
deploy/               # 部署模板（systemd unit + Windows nssm 脚本）
scripts/              # e2e 脚本（Python）+ OpenAPI 校验
docs/                 # 文档归档 + openapi.yaml + plantree 规划树
docker-compose.yml    # 本地 Postgres
```

## 快速开始

### 1. 启动 Postgres

```bash
docker compose up -d postgres
```

### 2. 启动 Server

```bash
cargo run -p helm-server
```

默认监听 HTTP `:8080`、gRPC `:50051`，首次启动自动迁移并 seed 管理员
`admin / admin123`。

### 3. 启动 Agent

```bash
cargo run -p helm-agent -- --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token-change-me
```

正向模式：`--conn-mode forward --listen-addr 0.0.0.0:50052`。
mTLS：Server 加 `--mtls`，Agent 加 `--cert-dir /tmp/agent-cert`。

### 4. 下发命令

```bash
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .token)

curl -s -X POST http://127.0.0.1:8080/api/v1/exec \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"agent_id":"my-host","command":"uname","args":["-a"]}'
```

## HTTP API（均需 `Authorization: Bearer <JWT>`）

完整契约见 [docs/openapi.yaml](docs/openapi.yaml)（OpenAPI 3.0.3）。

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 登录，换取 JWT（免认证） |
| POST | `/api/v1/agents/cert` | Agent 提交 CSR 换 mTLS 证书（免 JWT） |
| GET/POST | `/api/v1/hosts` | 列出（分页/标签过滤/在线状态）/ 创建主机 |
| PUT/DELETE | `/api/v1/hosts/{id}` | 更新 / 删除主机 |
| POST | `/api/v1/hosts/{id}/tags` | 设置主机标签 |
| GET | `/api/v1/agents` | 列出已注册 Agent |
| GET/DELETE | `/api/v1/agents/{id}` | Agent 详情 / 注销 |
| PUT | `/api/v1/agents/{id}/tags` | 更新 Agent 关联主机标签 |
| POST | `/api/v1/agents/{id}/uninstall` | 下发卸载指令 |
| POST | `/api/v1/exec` | 下发命令 |
| GET | `/api/v1/jobs` | 分页列出 Job |
| GET | `/api/v1/jobs/{id}` | 查询任务结果 |
| GET | `/api/v1/metrics?host_id=` | 查询主机指标 |
| GET | `/api/v1/alerts` | 分页列出告警 |
| POST | `/api/v1/files/upload` | 下发文件 |
| POST | `/api/v1/files/download` | 取回文件 |
| POST | `/api/v1/files/list` | 列目录 |
| POST | `/api/v1/tasks/script` | 脚本执行 |
| POST | `/api/v1/tasks/schedule` | 定时任务 |
| POST | `/api/v1/forward/exec` | 正向连接执行命令 |
| GET/POST | `/api/v1/listeners` | 列出 / 创建监听器 |
| PUT/DELETE | `/api/v1/listeners/{id}` | 更新 / 删除监听器 |
| POST | `/api/v1/listeners/{id}/start` | 启动监听器 |
| POST | `/api/v1/listeners/{id}/stop` | 停止监听器 |
| GET/POST | `/api/v1/services` | 分页列出 / 创建服务 |
| PUT/DELETE | `/api/v1/services/{id}` | 更新 / 删除服务 |
| POST | `/api/v1/services/{id}/start` | 启动服务 |
| POST | `/api/v1/services/{id}/stop` | 停止服务 |
| POST | `/api/v1/services/{id}/restart` | 重启服务 |
| GET | `/api/v1/services/{id}/logs` | 查询服务日志 |
| POST | `/api/v1/processes/list` | 列出进程 |
| POST | `/api/v1/processes/kill` | 杀进程 |
| POST | `/api/v1/net/info` | 网络信息 |
| GET | `/api/v1/audit` | 分页列出审计日志 |
| WS | `/api/v1/agents/{id}/terminal?token=` | 交互终端（PTY） |
| WS | `/api/v1/services/{id}/logs/stream?token=` | 服务日志实时流 |
| WS | `/api/v1/jobs/{id}/stream?token=` | job 输出实时流 |
| WS | `/api/v1/metrics/stream?token=` | 指标实时流 |

## 配置（环境变量）

### Server

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_HTTP_ADDR` | `0.0.0.0:8080` | HTTP 监听地址 |
| `HELM_GRPC_ADDR` | `0.0.0.0:50051` | gRPC 监听地址（Agent 反向连入） |
| `HELM_DATABASE_URL` | `postgres://helm:helm@localhost:5433/helm` | Postgres 连接串 |
| `HELM_SERVER_TOKEN` | `dev-token-change-me` | Agent 认证 token（严格匹配，空则拒绝所有；生产必须改） |
| `HELM_JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必须改） |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_HEARTBEAT_TIMEOUT` | `30` | 心跳超时阈值（秒） |
| `HELM_SESSION_IDLE_TIMEOUT` | `300` | 会话空闲超时（秒） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_MTLS` | 关 | 是否启用 mTLS（`--mtls`） |
| `HELM_TLS_DIR` | 空 | TLS 材料目录（持久化 CA，重启不换 CA；mTLS 部署强烈建议） |
| `HELM_ISSUE_CERT` | 关 | 离线签发 agent 证书三件套后退出（`--issue-cert`，forward 预置用） |
| `HELM_ISSUE_AGENT_ID` / `HELM_ISSUE_SAN` / `HELM_ISSUE_OUT_DIR` | 空 | issue-cert 参数：agent 标识 / SAN 列表 / 输出目录 |

### Agent

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_AGENT_ID` | 无（必填） | Agent 唯一标识 |
| `HELM_SERVER_ADDR` | `http://127.0.0.1:50051` | Server gRPC 地址 |
| `HELM_AGENT_TOKEN` | 空 | 注册 token |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_CONN_MODE` | `reverse` | 连接模式：reverse（主动连）/ forward（监听） |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | forward 模式监听地址 |
| `HELM_LOG_DIR` | 空 | 日志目录（非空按天滚动落文件） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_CERT_DIR` | 空 | 证书缓存目录（非空启用 mTLS） |
| `HELM_SERVER_HTTP_ADDR` | 空 | Server HTTP 地址（换证书用） |

## 开发

```bash
just check       # fmt + clippy + test 全部门禁
just buf-lint    # protobuf 契约 lint
python3 scripts/e2e-smoke.py      # 一键端到端 smoke
python3 scripts/e2e-phase8.py     # CRUD + 实时流（Phase 8）
python3 scripts/check_openapi.py  # OpenAPI 契约与路由一致性校验
```

## 文档

- [docs/](docs/) — 架构 / 技术栈 / API / 数据模型 / 运行部署 / 约定 / 现状归档。
- [docs/openapi.yaml](docs/openapi.yaml) — HTTP API 契约（OpenAPI 3.0.3）。
- [docs/plantree](docs/plantree/README.md) — 规划与架构决策树（决策 001–008 + roadmap Phase 0–8）。
