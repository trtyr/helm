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
  Windows 侧为内置原生能力：服务走 SCM、网络适配器/连接表走 IpHelper API、
  磁盘走 GetLogicalDrives、进程路径走 QueryFullProcessImageName——
  不依赖外部命令，无编码问题（监控数据）。
- **proto**（`proto/`）：gRPC 契约（protobuf），Server 与 Agent 的单一事实来源。

### 连接模式

- **反向**（默认）：Agent 主动连 Server，穿透 NAT/防火墙。
- **正向**：Agent 监听，Server 主动拨号（同区域内网）。

两种模式复用同一套信令协议（gRPC 双向流），见 engram「规划」分类中的架构决策（原 docs/plantree，决策 001–011）。

## IR 应急响应能力

以 Sysinternals Autoruns + Process Hacker + KAPE triage 为对标，内置完整应急响应能力：

| 能力 | 说明 |
|------|------|
| **自启动项全景** | 12 分类 1300+ 条（Run/RunOnce/服务/驱动/计划任务/浏览器/外壳/WMI/引导执行/已知 DLL/Winsock/编解码器/认证），签名校验 + 厂商识别 |
| **自启动项操作** | 禁用/启用/删除（AutorunsDisabled 机制），注册表/文件/服务/计划任务四类闭环 |
| **基线快照对比** | 保存扫描基线，diff 出新增/移除的持久化项 |
| **进程树** | 真实父子关系 + 折叠展开 + 异常父子高亮（办公派生解释器/LSASS 派生等 EDR 检测规则） |
| **流式内存扫描** | 全进程 / 单 PID，SeDebugPrivilege 提权，WebSocket 增量推送 |
| **NTFS USN 时间线** | 文件创建/删除/重命名实时活动记录 |
| **安全日志** | Security 事件 4624/4625/4720/1102/7045/4104（提权可读） |
| **批量操作** | 多主机同时下发命令，逐 agent 建任务跟踪 |
| **证据包** | 一键收集进程/网络/服务/自启动/日志 JSON 下载 |
| **权限检测** | 管理员/root 自动检测，前端徽章显示 |
| **页面缓存** | 自启动项/系统日志秒开（ir_page_cache 服务端缓存） |

详见 engram「应急响应」分类（原 docs/ir-capabilities.md）。

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
  migrations/         #   数据库迁移（21 个版本）
agent/                # Agent 被控端（tokio，跨平台）
  src/ir/             #   IR 应急响应模块（自启动/内存扫描/文件时间线/操作等 16 个模块）
  src/privilege.rs    #   权限检测（SeDebugPrivilege / Administrators 组）
console/              # 前端控制台（Vite + React；pnpm 独立工作流）
deploy/               # 部署模板（systemd unit + Windows nssm 脚本）
scripts/              # e2e 脚本（Python）+ OpenAPI 校验
docs/                 # openapi.yaml（机器契约）+ README.md（engram 文档指针索引）；归档正文已迁 engram
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

### 3. 接入主机（生成 Agent → 运行上线）

主机不需要手工创建——**Agent 上线即注册**。两种接入方式：

- **生成 Agent（推荐）**：控制台「主机 → 生成 Agent」选择监听器与目标平台
  （Windows / Linux / macOS），Server 现场交叉编译并把连入地址与注册 token 烙入二进制
  （`agent_id` 不烙入，目标机首跑按主机名自动生成，一份二进制通吃同平台主机）。
  下载后拷到目标机直接运行（Linux/macOS 需 `chmod +x`），无需任何参数即可上线。
- **本地开发**：`cargo run -p helm-agent -- --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token-change-me`

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

## HTTP API（均需 `Authorization: Bearer <JWT 或 API key>`；api-keys 管理端点仅 JWT）

Bearer 值为 JWT 或 `helm_` 前缀 API key（决策 010）均可认证；WebSocket `?token=` 同理。
完整契约见 [docs/openapi.yaml](docs/openapi.yaml)（OpenAPI 3.0.3）。

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/auth/login` | 登录，换取 JWT（免认证） |
| GET | `/api/v1/auth/me` | 当前账号（仅 JWT；单用户） |
| POST | `/api/v1/auth/change-password` | 修改密码（校验当前密码，新密码 ≥ 6 字符） |
| POST | `/api/v1/auth/change-username` | 修改用户名（校验当前密码，旧 token 随即失效） |
| POST | `/api/v1/agents/cert` | Agent 提交 CSR 换 mTLS 证书（免 JWT） |
| GET/POST | `/api/v1/hosts` | 列出（分页/标签过滤/在线状态）/ 创建主机 |
| GET/PUT/DELETE | `/api/v1/hosts/{id}` | 查询单台 / 更新 / 删除主机 |
| POST | `/api/v1/hosts/{id}/tags` | 设置主机标签 |
| GET | `/api/v1/agents` | 列出已注册 Agent |
| GET/DELETE | `/api/v1/agents/{id}` | Agent 详情 / 注销 |
| PUT | `/api/v1/agents/{id}/tags` | 更新 Agent 关联主机标签 |
| POST | `/api/v1/agents/{id}/uninstall` | 下发卸载指令 |
| GET/POST | `/api/v1/agent-gen` | 列出 / 创建 Agent 现场编译任务（选监听器 + 目标平台，异步 cargo 交叉编译） |
| GET | `/api/v1/agent-gen/{id}` | 生成任务进度（含编译日志尾部） |
| GET | `/api/v1/agent-gen/{id}/download` | 下载编译完成的 Agent 二进制 |
| POST | `/api/v1/exec` | 下发命令 |
| GET | `/api/v1/jobs` | 分页列出 Job（`?status=&host_id=` 过滤） |
| GET | `/api/v1/jobs/{id}` | 查询任务结果 |
| GET | `/api/v1/metrics?host_id=` | 查询主机指标 |
| GET | `/api/v1/alerts` | 分页列出告警 |
| GET | `/api/v1/notifications` | 系统内通知列表（上线/下线/预警，`unread=` 过滤） |
| GET | `/api/v1/notifications/unread-count` | 未读通知数 |
| POST | `/api/v1/notifications/{id}/read` | 标记通知已读 |
| POST | `/api/v1/notifications/read-all` | 全部通知已读 |
| GET/POST | `/api/v1/api-keys` | 分页列出 / 创建 API key（**明文 key 仅创建响应返回一次**） |
| GET/DELETE | `/api/v1/api-keys/{id}` | API key 详情 / 吊销（仅 JWT，key 不可自管） |
| POST | `/api/v1/files/upload` | 下发文件 |
| POST | `/api/v1/files/download` | 取回文件 |
| POST | `/api/v1/files/list` | 列目录 |
| POST | `/api/v1/tasks/script` | 脚本执行 |
| POST | `/api/v1/tasks/schedule` | 定时任务 |
| POST | `/api/v1/mcp` | MCP JSON-RPC 端点（AI 单工具 `helm` 接入，41 op，scope 授权；见 engram「接口契约/mcp」） |
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
| POST | `/api/v1/processes/list` | 列出进程（CPU/内存/属主/父进程/命令行） |
| POST | `/api/v1/processes/kill` | 杀进程 |
| POST | `/api/v1/net/info` | 网络信息（接口 + TCP/UDP 连接表，含归属进程） |
| POST | `/api/v1/sys-services/list` | 枚举目标机系统服务（Windows Service / systemd / launchctl） |
| POST | `/api/v1/sys-services/action` | 系统服务操作（start / stop / restart） |
| GET | `/api/v1/audit` | 分页列出审计日志 |
| WS | `/api/v1/agents/{id}/terminal?token=` | 交互终端（PTY） |
| WS | `/api/v1/services/{id}/logs/stream?token=` | 服务日志实时流 |
| WS | `/api/v1/services/stream?token=` | 服务状态实时流（连接推全量快照，此后状态变更增量） |
| WS | `/api/v1/jobs/{id}/stream?token=` | job 输出实时流 |
| WS | `/api/v1/metrics/stream?token=` | 指标实时流 |
| WS | `/api/v1/notifications/stream?token=` | 通知实时流（上线/下线/预警） |
| WS | `/api/v1/ir/memscan/{id}/stream?token=` | 内存扫描实时流（流式进度与命中） |

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
| `HELM_AGENT_SOURCE_DIR` | `.` | Agent 源码工作区目录（「生成 Agent」现场编译用；须能在此目录执行 cargo） |
| `HELM_CROSS_TOOLS_DIR` | 未设 | musl 交叉工具链目录（缺省 `<源码工作区>/.cargo-musl/bin`） |

### Agent

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_AGENT_ID` | 空 | Agent 唯一标识（缺省取编译期烙入值，再退回主机名） |
| `HELM_SERVER_ADDR` | `http://127.0.0.1:50051` | Server gRPC 地址（可编译期烙入） |
| `HELM_AGENT_TOKEN` | 空 | 注册 token |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_CONN_MODE` | `reverse` | 连接模式：reverse（主动连）/ forward（监听） |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | forward 模式监听地址 |
| `HELM_LOG_DIR` | 空 | 日志目录（非空按天滚动落文件） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_CERT_DIR` | 空 | 证书缓存目录（非空启用 mTLS） |
| `HELM_SERVER_HTTP_ADDR` | 空 | Server HTTP 地址（换证书用；缺省按 gRPC 地址同 host + 8080 推导，与 Server 默认 HTTP 端口一致；非默认部署请显式设置） |

> 编译期烙入变量（「生成 Agent」现场编译注入，`agent/build.rs` 白名单）：`HELM_BAKE_SERVER_ADDR` / `HELM_BAKE_AGENT_TOKEN` / `HELM_BAKE_AGENT_ID` / `HELM_BAKE_CONN_MODE` / `HELM_BAKE_LISTEN_ADDR`。运行时优先级：CLI 参数 > 环境变量 > 编译期烙入 > 内置兜底。

## 开发

```bash
just check       # fmt + clippy + test 全部门禁
just buf-lint    # protobuf 契约 lint
python3 scripts/e2e-smoke.py      # 一键端到端 smoke
python3 scripts/e2e-phase8.py     # CRUD + 实时流（Phase 8）
python3 scripts/e2e-phase9.py     # 通知中心（Phase 9）
python3 scripts/e2e-mcp.py        # MCP 端到端（签发→握手→裁剪→透传→越权→吊销）
python3 scripts/e2e-account.py    # 单用户账号管理（Phase 12）
python3 scripts/check_openapi.py  # OpenAPI 契约与路由一致性校验
python3 scripts/check_docs.py     # 文档现状陈述与代码一致性校验
```

## 文档

- **engram（projects/helm）** — 全部书面记录的唯一归档地：架构 / 技术栈 / API / 数据模型 /
  运行部署 / 约定 / 现状与门禁 / 应急响应 / 规划决策树（原 docs/*.md 与 docs/plantree 全树，
  2026-09-15 迁入）。经 engram MCP（`projects` 工具 `doc_search` / `doc_get`，project_name=helm）检索。
- [docs/README.md](docs/README.md) — 本地文档指针索引（分类 → engram 文档对照表）。
- [docs/openapi.yaml](docs/openapi.yaml) — HTTP API 契约（OpenAPI 3.0.3，72 端点；机器契约保留本地）。
