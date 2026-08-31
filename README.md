# helm — 集中式运维平台

集中式运维平台后端：**Server 控制端** + 跨平台 **Agent 被控端**。
下发 Agent 到目标主机（Windows / Linux / macOS），即可对该主机进行远程运维。

## 架构

```text
[控制台 / HTTP API] ──► Server 控制端 ◄──gRPC 双向流──► Agent 被控端（各目标主机）
```

- **Server**（`server/`）：集中管理、任务编排、状态收集、持久化（Postgres）、
  对外 HTTP API。
- **Agent**（`agent/`）：装在目标机，执行命令、采集指标、文件传输。单二进制，跨平台。
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
  migrations/         #   数据库迁移
agent/                # Agent 被控端（tokio，跨平台）
scripts/e2e-smoke.sh  # 一键端到端 smoke
docker-compose.yml    # 本地 Postgres
docs/plantree/        # 规划与架构决策树
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
cargo run -p helm-agent -- --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token
```

正向模式：`--conn-mode forward --listen-addr 0.0.0.0:50052`。

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

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 登录，换取 JWT（免认证） |
| GET  | `/api/v1/hosts` | 列出主机 |
| POST | `/api/v1/exec` | 下发命令 |
| GET  | `/api/v1/jobs/{id}` | 查询任务结果 |
| GET  | `/api/v1/metrics?host_id=` | 查询主机指标 |
| POST | `/api/v1/files/upload` | 下发文件 |
| POST | `/api/v1/files/download` | 取回文件 |
| POST | `/api/v1/tasks/script` | 脚本执行 |
| POST | `/api/v1/tasks/schedule` | 定时任务 |
| POST | `/api/v1/forward/exec` | 正向连接执行命令 |

## 配置（环境变量）

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_HTTP_ADDR` | `0.0.0.0:8080` | Server HTTP 监听地址 |
| `HELM_GRPC_ADDR` | `0.0.0.0:50051` | Server gRPC 监听地址 |
| `HELM_DATABASE_URL` | `postgres://helm:helm@localhost:5433/helm` | Postgres 连接串 |
| `HELM_SERVER_TOKEN` | 空（放行） | Agent 认证 token |
| `HELM_JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必须改） |
| `HELM_CONN_MODE` | `reverse` | Agent 连接模式 |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | Agent forward 监听地址 |

## 开发

```bash
just check       # fmt + clippy + test 全部门禁
just buf-lint    # protobuf 契约 lint
./scripts/e2e-smoke.sh   # 一键端到端 smoke
```

## 架构决策

底座先行、一次到位。关键决策（gRPC 底座、正/反向同构、Postgres、分层架构）
记录在 [docs/plantree](docs/plantree/README.md)。
