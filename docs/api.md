# API — 对外接口契约

接口分两层：**gRPC**（Server ↔ Agent 控制信道）与 **HTTP**（控制台消费）。
两者契约的单一事实来源是 `proto/helm/agent/v1/`（gRPC）与 `docs/openapi.yaml`（HTTP，OpenAPI 3.0.3）。

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
| `session_opened` | SessionOpened | 会话已打开 |
| `session_output` | SessionOutput | 会话输出（PTY 读到的字节） |
| `session_closed` | SessionClosed | 会话已关闭（附退出码） |
| `service_status` | ServiceStatus | 常驻服务状态 + 增量日志 |
| `file_list_result` | FileListResult | 目录内容 |
| `process_list_result` | ProcessListResult | 进程列表 |
| `process_kill_result` | ProcessKillResult | 终止结果 |
| `net_info_result` | NetInfoResult | 网络信息 |

**ServerMessage**（Server → Agent），`oneof kind`：

| 字段 | 类型 | 含义 |
|------|------|------|
| `register_ack` | RegisterAck | 注册应答（ok, heartbeat_interval_secs） |
| `exec_request` | ExecRequest | 命令下发（job_id, command, args, timeout, working_dir） |
| `file_request` | FileRequest | 文件传输指令（direction, path, size, chunk_size） |
| `metric_request` | MetricRequest | 触发一次采集（当前为空消息） |
| `file_chunk` | FileChunk | 文件数据块（upload 方向） |
| `self_destruct` | SelfDestruct | 卸载/自杀指令（remove_binary） |
| `session_open` | SessionOpen | 打开会话（cols, rows, command） |
| `session_input` | SessionInput | 会话输入（写入 PTY 的字节） |
| `session_close` | SessionClose | 关闭会话 |
| `session_resize` | SessionResize | 调整终端窗口大小 |
| `service_start` | ServiceStart | 启动常驻服务（command, args, restart_policy） |
| `service_stop` | ServiceStop | 停止常驻服务 |
| `file_list` | FileList | 列目录（request_id, path） |
| `process_list` | ProcessList | 列出进程（request_id） |
| `process_kill` | ProcessKill | 终止进程（request_id, pid） |
| `net_info` | NetInfo | 采集网络信息（request_id） |

### 关键类型（`types.proto`）

- `HostInfo`：hostname / os / arch / platform / tags。
- `MetricPoint`：name / value / labels(map) / timestamp_unix_ms。
- `StreamChunk`：stdout/stderr 分块。
- `FileStatus.State`：pending → transferring → done | failed。
- `FileRequest.Direction`：upload（Server→Agent）/ download（Agent→Server）。
- `FileEntry`：name / is_dir / size / modified_unix_ms / mode。
- `ProcessInfo`：pid / name / cpu_percent / mem_bytes。
- `NetInterface`：name / addrs[]。

### 请求-应答关联约定

Server 主动发起的查询（列目录 / 进程 / 网络）用 `request_id`（UUID）关联：
Server 注册 `oneshot` → 下发 `FileList`/`ProcessList`/`ProcessKill`/`NetInfo` → Agent 回
`*Result` → Server 完成 oneshot。对应 registry：`FileListRegistry` / `QueryRegistry`。

### 认证

- Agent 注册 token 由 `Register.token` 携带，Server 用 `HELM_SERVER_TOKEN` **严格匹配**，
  且 server_token 为空时拒绝所有（`token_matches` 纯函数）。
- 首条消息必须是 `Register`，否则 `Status::invalid_argument`；token 不匹配返回 `Status::unauthenticated`。
- 可选 mTLS，取证路径按连接模式分两条：
  - **反向**：Agent 经 `POST /api/v1/agents/cert` 提交 CSR（token 认证）换证书后缓存本地，
    双向 gRPC 走 TLS 双向认证（Server `--mtls` 开关）。
  - **正向**：Agent 不回连，证书由管理员在 Server 侧 `helm-server --issue-cert` 用持久 CA
    （`--tls-dir`）离线签发三件套后带外预置到 Agent `--cert-dir`；Agent 以 mTLS 监听、
    Server 以 `ClientTlsConfig` 主动拨号（详见 [run-and-deploy.md](run-and-deploy.md) 3.1 节）。

## 2. HTTP API（`server/src/http/`）

前缀 `/api/v1`，除 `auth/login`、`agents/cert`、`/healthz` 与 WebSocket 端点外均需
`Authorization: Bearer <JWT>`。完整契约见 [openapi.yaml](openapi.yaml)。

### 认证与健康

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/healthz` | 健康探针（免认证） |
| POST | `/api/v1/auth/login` | 登录换 JWT（免认证） `{username, password}` → `{token}` |
| POST | `/api/v1/agents/cert` | Agent 提交 CSR 换 mTLS 证书（token 放 body，免 JWT） `{agent_id, token, csr_pem}` → `{cert_pem, ca_cert_pem}` |

### 主机

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/hosts?tag=&page=&limit=` | 列出主机（附 online/last_seen/stale，支持标签过滤 + 分页） |
| POST | `/api/v1/hosts` | 创建主机（reverse/forward） `{hostname, conn_mode, addr, tags}` → `{host}` |
| PUT | `/api/v1/hosts/{id}` | 更新主机 `{hostname, conn_mode, addr, tags, os, arch, platform}` → `{host}` |
| DELETE | `/api/v1/hosts/{id}` | 删除主机（软删除） |
| POST | `/api/v1/hosts/{id}/tags` | 设置标签 `{tags}` → `{host}` |

### Agent 生命周期

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/agents` | 列出已注册 Agent |
| GET | `/api/v1/agents/{id}` | Agent 详情（附在线状态） |
| DELETE | `/api/v1/agents/{id}` | 注销（删 agent + 孤儿主机软删） |
| PUT | `/api/v1/agents/{id}/tags` | 更新关联主机标签 `{tags}` → `{host}` |
| POST | `/api/v1/agents/{id}/uninstall` | 下发卸载指令（SelfDestruct） `{remove_binary}` → `{ok}`（离线返回 409） |

### 命令执行与 Job

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/exec` | 下发命令 `{agent_id, command, args}` → `{job_id}` |
| GET | `/api/v1/jobs?page=&limit=` | 分页列出 Job |
| GET | `/api/v1/jobs/{id}` | 查询任务结果 `{job}` |

### 指标与告警

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/metrics?host_id=&limit=` | 查询主机指标（时序） |
| GET | `/api/v1/alerts?page=&limit=` | 分页列出告警 |

### 文件

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/files/upload` | 下发文件 `{agent_id, local_path, remote_path}` → `{checksum_ok}` |
| POST | `/api/v1/files/download` | 取回文件 `{agent_id, remote_path, local_path}` → `{checksum_ok}` |
| POST | `/api/v1/files/list` | 列目录 `{agent_id, path}` → `{path, entries}` |

### 任务与正向连接

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/tasks/script` | 脚本执行（复用 exec） `{agent_id, command, args}` → `{job_id}` |
| POST | `/api/v1/tasks/schedule` | 定时任务 `{agent_id, command, args, interval_secs}` → `{task_id}` |
| POST | `/api/v1/forward/exec` | 正向按需拨号执行 `{hostname?\|agent_addr?, command, args}` → `{output, exit_code}`（临时连接的旧路径；forward host 持久连接后，其余端点按 agent_id 直接可用） |

### 监听器

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/listeners` | 列出监听器 |
| POST | `/api/v1/listeners` | 创建监听器（默认 stopped） `{name, addr, proto, auth}` → `{listener}` |
| PUT | `/api/v1/listeners/{id}` | 更新监听器 → `{listener}` |
| DELETE | `/api/v1/listeners/{id}` | 删除监听器 |
| POST | `/api/v1/listeners/{id}/start` | 启动监听器（动态绑定 gRPC） |
| POST | `/api/v1/listeners/{id}/stop` | 停止监听器 |

### 常驻服务

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/services?page=&limit=` | 分页列出服务 |
| POST | `/api/v1/services` | 创建服务 `{agent_id, name, command, args, restart_policy}` → `{service}` |
| PUT | `/api/v1/services/{id}` | 更新服务 `{name, command, args, restart_policy}` → `{service}` |
| DELETE | `/api/v1/services/{id}` | 删除服务（running 需先 stop） |
| POST | `/api/v1/services/{id}/start` | 启动服务 |
| POST | `/api/v1/services/{id}/stop` | 停止服务 |
| POST | `/api/v1/services/{id}/restart` | 重启服务（stop + start） |
| GET | `/api/v1/services/{id}/logs` | 查询服务日志（快照） `{log}` |

### 进程与网络

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/processes/list` | 列出进程 `{agent_id}` → `{processes}` |
| POST | `/api/v1/processes/kill` | 杀进程 `{agent_id, pid}` → `{ok}`（不存在 pid 返回 ok=false 仍 200） |
| POST | `/api/v1/net/info` | 网络信息 `{agent_id}` → `{hostname, interfaces}` |

### 审计

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/audit?page=&limit=` | 分页列出审计日志 |

### WebSocket 实时流（query-param token 鉴权）

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/v1/agents/{id}/terminal?token=` | 交互终端（PTY 双向流） |
| GET | `/api/v1/services/{id}/logs/stream?token=` | 服务日志实时流（二进制帧 = 日志增量） |
| GET | `/api/v1/jobs/{id}/stream?token=` | job 输出实时流（二进制帧 = 输出增量） |
| GET | `/api/v1/metrics/stream?token=` | 指标实时流（二进制帧 = JSON 指标点） |

WebSocket 握手无法携带 `Authorization` header，故鉴权走 query-param `token`（JWT），
由 `AuthService::verify` 直接校验，绕开 JWT 中间件（挂顶层路由）。

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
| `HELM_SERVER_TOKEN` | `dev-token-change-me` | Agent 认证 token（严格匹配，空则拒绝；生产必改） |
| `HELM_JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必改） |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_HEARTBEAT_TIMEOUT` | `30` | 心跳超时阈值（秒，30 = 3×心跳间隔） |
| `HELM_SESSION_IDLE_TIMEOUT` | `300` | 会话空闲超时（秒） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_MTLS` | 关 | 是否启用 mTLS（`--mtls` 开关） |
| `HELM_TLS_DIR` | 空 | TLS 材料目录（持久化 CA/server 证书，重启不换 CA；mTLS 部署强烈建议） |
| `HELM_ISSUE_CERT` | 关 | 离线签发 agent 证书三件套后退出（forward 预置分发用，不启动服务） |
| `HELM_ISSUE_AGENT_ID` | 空 | issue-cert：agent 唯一标识（写入证书 CN） |
| `HELM_ISSUE_SAN` | 空 | issue-cert：SAN 列表，逗号分隔 DNS/IP |
| `HELM_ISSUE_OUT_DIR` | 空 | issue-cert：三件套输出目录（cert.pem/key.pem/ca.pem） |

**Agent**（`agent/src/config/mod.rs`）：

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_AGENT_ID` | 无（必填） | Agent 唯一标识 |
| `HELM_SERVER_ADDR` | `http://127.0.0.1:50051` | Server gRPC 地址 |
| `HELM_AGENT_TOKEN` | 空 | 注册 token |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_CONN_MODE` | `reverse` | reverse（主动连）/ forward（监听） |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | forward 模式监听地址 |
| `HELM_LOG_DIR` | 空 | 日志目录（非空按天滚动落文件） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_CERT_DIR` | 空 | 证书缓存目录（非空启用 mTLS） |
| `HELM_SERVER_HTTP_ADDR` | 空 | Server HTTP 地址（换证书用，默认从 server_addr 推导） |

> 注：根 `README.md` 的配置表已拆为 Server/Agent 两表，并与本节对齐。
