# Run & Deploy — 运行与部署

## 前置条件

- Rust toolchain（实测 1.98.0）
- Docker（起本地 Postgres）
- [Just](https://github.com/casey/just)（可选，命令入口）
- [buf](https://buf.build)（契约 lint，可选）
- Python 3（e2e 脚本；实时流 e2e 需 `websockets` 库）

## 本地运行

### 1. 启动 Postgres

```bash
docker compose up -d postgres
# 或：just db-up
```

容器 `helm-postgres`，宿主机 `localhost:5433`（映射容器 5432），库/用户/密码均为 `helm`。

### 2. 启动 Server

```bash
cargo run -p helm-server
# 或：just run-server
```

- 默认 HTTP `:8080`、gRPC `:50051`。
- 首次启动自动执行迁移（7 个版本），并 seed 管理员 `admin / admin123`（仅当 users 表为空）。
- 首次启动自动 seed 默认监听器（`listeners` 表为空时），后续启动恢复 running 的监听器。
- 启用 mTLS：`cargo run -p helm-server -- --mtls --tls-dir /var/lib/helm-tls`。
  `--tls-dir` 持久化 CA 与 Server 证书（`ca.pem`/`ca-key.pem`/`server.pem`/`server-key.pem`），
  首次自动生成、重启复用——**不换 CA，Agent 缓存证书长期有效**；省略则每次启动随机生成 CA（仅测试用）。

### 3. 启动 Agent

```bash
# 反向模式（默认）
cargo run -p helm-agent -- --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token-change-me

# 正向模式（明文）
cargo run -p helm-agent -- --agent-id my-host --conn-mode forward --listen-addr 0.0.0.0:50052

# 反向模式 + mTLS（--cert-dir 非空则启用，首次用 token 经 HTTP 换证书并缓存）
cargo run -p helm-agent -- --agent-id my-host --server-addr https://127.0.0.1:50051 --token dev-token-change-me --cert-dir /tmp/agent-cert

# 正向模式 + mTLS（证书须预先签发好放到 --cert-dir，见下一节；agent 不回连、无 HTTP 换证书路径）
cargo run -p helm-agent -- --agent-id my-host --conn-mode forward --listen-addr 0.0.0.0:50052 \
  --token dev-token-change-me --cert-dir /var/lib/helm-agent-cert
```

### 3.1 forward mTLS 证书预置（管理员分发）

forward 模式的 Agent 只监听、不回连 Server，无法走「token 换证书」的 HTTP 路径，因此证书由
**管理员在 Server 侧离线签发后手工放置**：

```bash
# 1. 在 Server 机器上用持久 CA（--tls-dir）签出 Agent 证书三件套到 out-dir，然后退出（不启动服务）
helm-server --issue-cert \
  --issue-agent-id my-fwd-host \
  --issue-san localhost `# Agent 证书 SAN，须与 Server 拨号时的 --tls-server-name 一致` \
  --issue-out-dir /tmp/fwd-cert \
  --tls-dir /var/lib/helm-tls --mtls
# 产出：/tmp/fwd-cert/{cert.pem, key.pem, ca.pem}

# 2. 将三件套分发到 Agent 机器（scp/U盘等任意带外方式）
scp /tmp/fwd-cert/*.pem admin@agent-host:/var/lib/helm-agent-cert/

# 3. Agent 以 --cert-dir 指向该目录启动（缺任一文件将拒绝启动并提示补齐）
helm-agent --agent-id my-fwd-host --conn-mode forward --listen-addr 0.0.0.0:50052 \
  --cert-dir /var/lib/helm-agent-cert

# 4. Server 侧 --mtls --tls-dir 后，为该主机建 forward host（addr=agent 的 ip:50052），
#    reconciler 自动以 mTLS 拨号（domain_name=--tls-server-name，默认 localhost）
```

要点：SAN 与拨号域名的对应关系——Server `--tls-server-name`（默认 `localhost`）必须命中 Agent
证书的 SAN，否则 TLS 校验失败；公网 IP 直连场景用默认 `localhost` SAN 即可（拨号侧显式指定域名，
不依赖实际 IP）。CA 持久化（`--tls-dir`）是前提：CA 变更则所有已分发证书作废。

### 4. 下发命令（完整示例）

```bash
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .token)

curl -s -X POST http://127.0.0.1:8080/api/v1/exec \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"agent_id":"my-host","command":"uname","args":["-a"]}'
```

## 开发命令（Justfile）

| 命令 | 作用 |
|------|------|
| `just check` | 全部门禁：`fmt-check` + `lint` + `test` |
| `just fmt` / `just fmt-check` | `cargo fmt` / `-- --check` |
| `just lint` | `cargo clippy --all-targets -- -D warnings` |
| `just test` | `cargo test` |
| `just buf-lint` | `buf lint` |
| `just buf-breaking` | `buf breaking --against '.git#ref=HEAD~1'` |
| `just db-up` / `db-down` | 启停 Postgres |
| `just run-server` / `run-agent` | 运行 server / agent |

## 端到端 e2e（Python，纯标准库）

```bash
python3 scripts/e2e-smoke.py       # 一键：Postgres + Server + Agent → 登录 → 下发命令 → 验证 succeeded
python3 scripts/e2e-scheduler.py   # 定时任务持久化恢复：建 schedule → 重启 Server → 验证恢复执行
python3 scripts/e2e-phase5.py      # 监听器启停 + hosts 在线状态 + agent 注销/卸载
python3 scripts/e2e-phase6.py      # 会话终端 + 服务管理 + 文件 + 进程/网络 + 分组标签
python3 scripts/e2e-phase7.py      # mTLS 握手 + 审计落库 + 告警端点
python3 scripts/e2e-phase8.py      # CRUD 补全 + 三个实时流（WS）
```

共享工具在 `scripts/e2e_helpers.py`；HTTP 地址用 `HELM_E2E_HTTP_ADDR`（默认 `127.0.0.1:18080`）、
gRPC 用 `HELM_E2E_GRPC_ADDR`（默认 `127.0.0.1:50051`）覆盖。
e2e-phase6/phase8 需要第三方 `websockets`（Python 标准库无 WebSocket 客户端）。

## 部署（服务化 + 自启动）

`deploy/` 提供平台模板：

- **Linux**：`deploy/helm-agent.service` — systemd unit（`Type=simple`，`Restart=always`，
  `ExecStart=/usr/local/bin/helm-agent ... --log-dir /var/log/helm-agent`）。
  安装：复制到 `/etc/systemd/system/`，`systemctl enable --now helm-agent`。
- **Windows**：`deploy/install-windows-service.ps1` — nssm 安装器（需管理员 PowerShell + nssm 在 PATH），
  配置 `AppStdout`/`AppStderr` 落文件，`SERVICE_AUTO_START` 自启动。
  Agent 编译时已带 `#![cfg_attr(windows, windows_subsystem = "windows")]` 去除黑窗口。

Agent 单二进制跨平台交叉编译（在 macOS 上实测）：

```bash
# Linux（musl 静态）
CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=x86_64-linux-musl-gcc \
  cargo build --release -p helm-agent --target x86_64-unknown-linux-musl

# Windows（GNU，无 MinGW DLL 依赖）
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release -p helm-agent --target x86_64-pc-windows-gnu
```

## 发布门禁

合并前须通过（见 `docs/plantree/baseline/test-and-release-gates.md`）：

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. `buf lint`（契约风格）+ `buf breaking --against '.git#ref=HEAD~1'`（契约兼容）
5. `python3 scripts/check_openapi.py`（HTTP 契约与 server 路由一致）
6. 无密钥/敏感信息进 diff

## 部署要点

- **生产必改**：`HELM_SERVER_TOKEN`、`HELM_JWT_SECRET` 两个默认值均为 `dev-*-change-me`。
- 无 CI 配置文件（无 `.github/workflows`）；门禁当前为本地 `just check` + 手动 `buf` 检查。
- 反向模式 Agent 穿透 NAT；正向模式需 Server 能直达 Agent 的 `HELM_LISTEN_ADDR`。
- mTLS 两条路径不同：**reverse** = Server `--mtls` + Agent `--cert-dir`（首次用 token 经 HTTP 换证书并缓存）；
  **forward** = Server `--mtls --tls-dir` + Agent 证书**管理员预置**（`helm-server --issue-cert` 签发后
  手工放置，Agent 仅读盘、无 HTTP 回退），见上文 3.1 节。
