# API — 对外接口契约

接口分两层：**gRPC**（Server ↔ Agent 控制信道）与 **HTTP**（控制台消费）。两者契约的单一事实来源是 `proto/helm/agent/v1/`。

## 1. gRPC 契约（`proto/helm/agent/v1/`）

### 服务定义

```proto
// 反向模式：Agent 作为 client 拨号 Server，调用 OpenChannel 建立双向流。
service AgentService {
  rpc OpenChannel(stream AgentMessage) returns (stream ServerMessage);
}

// 正向模式：Agent 作为 server 监听，Server 作为 client 拨号调用。
service ForwardAgentService {
  rpc OpenForwardChannel(stream ServerMessage) returns (stream AgentMessage);
}
```

正/反向共用同一套 envelope（决策 002「信令协议同构」），只交换收发方向。

### 消息 envelope

**AgentMessage**（Agent → Server），`oneof kind`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `register` | Register | 首条：注册（agent_id, token, host, version） |
| `heartbeat` | Heartbeat | 心跳（timestamp_unix_ms） |
| `metric_report` | MetricReport | 指标批量上报 |
| `exec_result` | ExecResult | 命令执行结果（分块流式） |
| `file_chunk` | FileChunk | 文件数据块（download 方向） |
| `file_status` | FileStatus | 文件传输状态 + 校验和 |

**ServerMessage**（Server → Agent），`oneof kind`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `register_ack` | RegisterAck | 注册应答（ok, heartbeat_interval_secs） |
| `exec_request` | ExecRequest | 命令下发（job_id, command, args, timeout, working_dir） |
| `file_request` | FileRequest | 文件传输指令（direction, path, size, chunk_size） |
| `metric_request` | MetricRequest | 触发一次采集（当前为空消息） |
| `file_chunk` | FileChunk | 文件数据块（upload 方向） |

### 关键类型（`types.proto`）

- `HostInfo`：hostname / os / arch / platform / tags。
- `MetricPoint`：name / value / labels(map) / timestamp_unix_ms。
- `StreamChunk`：stdout/stderr 分块。
- `FileStatus.State`：pending → transferring → done | failed。
- `FileRequest.Direction`：upload（Server→Agent）/ download（Agent→Server）。

### 认证

- Agent 注册 token 由 `Register.token` 携带，Server 用 `HELM_SERVER_TOKEN` **严格匹配**，
  且 server_token 为空时拒绝所有（`token_matches` 纯函数）。
- 首条消息必须是 `Register`，否则 `Status::invalid_argument`；token 不匹配返回 `Status::unauthenticated`。

## 2. HTTP API（`server/src/http/`）

前缀 `/api/v1`，除 `auth/login` 与 `/healthz` 外均需 `Authorization: Bearer <JWT>`。

| 方法 | 路径 | 说明 | 请求体 |
|------|------|------|--------|
| GET | `/healthz` | 健康探针（免认证） | — |
| POST | `/api/v1/auth/login` | 登录换 JWT（免认证） | `{username, password}` → `{token}` |
| GET | `/api/v1/hosts` | 列出主机 | — → `{hosts}` |
| POST | `/api/v1/hosts` | 创建主机（反向/正向） | `{hostname, conn_mode, addr, tags}` → `{host}` |
| POST | `/api/v1/exec` | 下发命令 | `{agent_id, command, args}` → `{job_id}` |
| GET | `/api/v1/jobs/{id}` | 查询任务结果 | — → `{job}` |
| GET | `/api/v1/metrics?host_id=&limit=` | 查询主机指标 | — → `{metrics}` |
| POST | `/api/v1/files/upload` | 下发文件到目标机 | `{agent_id, local_path, remote_path}` → `{transfer_id, checksum_ok}` |
| POST | `/api/v1/files/download` | 从目标机取文件 | `{agent_id, remote_path, local_path}` → `{transfer_id, checksum_ok}` |
| POST | `/api/v1/tasks/script` | 脚本执行（复用 exec） | `{agent_id, command, args}` → `{job_id}` |
| POST | `/api/v1/tasks/schedule` | 定时任务 | `{agent_id, command, args, interval_secs}` → `{task_id}` |
| POST | `/api/v1/forward/exec` | 正向连接执行 | `{hostname?|agent_addr?, command, args}` → `{output, exit_code}` |

### 认证

- `POST /api/v1/auth/login`：校验密码（bcrypt），签发 JWT（HS256，24h 过期），claims = `{sub: username, role, exp}`。
- 其余受保护路由经 `require_auth` 中间件：解析 `Authorization: Bearer`，校验后把 `Claims` 塞进 request extension。

### 错误格式

统一由 `server/src/http/error.rs` 的 `IntoResponse for Error` 输出：

```json
{ "error": { "code": "not_found", "message": "resource not found" } }
```

- 稳定 `code`：`not_found` / `unauthorized` / `invalid_argument` / `not_connected` / `storage` / `io` / `internal`。
- HTTP 状态映射：NotFound→404，Unauthorized→401，InvalidArgument→400，NotConnected→409，其余→500。
- 内部错误细节只进 tracing 日志，`message` 用 `safe_message()`（不外泄 DB/IO 内部串）。

## 3. 配置（环境变量 / CLI）

**Server**（`server/src/config/mod.rs`，clap derive，均可用 `--` 长参数或 env）：

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_HTTP_ADDR` | `0.0.0.0:8080` | HTTP 监听地址 |
| `HELM_GRPC_ADDR` | `0.0.0.0:50051` | gRPC 监听地址 |
| `HELM_DATABASE_URL` | `postgres://helm:helm@localhost:5433/helm` | Postgres 连接串 |
| `HELM_SERVER_TOKEN` | `dev-token-change-me` | Agent 认证 token（生产必改） |
| `HELM_JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必改） |
| `HELM_LOG` | `info` | 日志级别 |

**Agent**（`agent/src/config/mod.rs`）：

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_AGENT_ID` | 无（必填） | Agent 唯一标识 |
| `HELM_SERVER_ADDR` | `http://127.0.0.1:50051` | Server gRPC 地址 |
| `HELM_AGENT_TOKEN` | 空 | 注册 token |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_CONN_MODE` | `reverse` | reverse（主动连）/ forward（监听） |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | forward 模式监听地址 |

> 注：根 `README.md` 的配置表已拆为 Server/Agent 两表，并与本节对齐（2026-09 修正）。
