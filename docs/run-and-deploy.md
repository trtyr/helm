# Run & Deploy — 运行与部署

## 前置条件

- Rust toolchain（实测 1.98.0）
- Docker（起本地 Postgres）
- [Just](https://github.com/casey/just)（可选，命令入口）
- [buf](https://buf.build)（契约 lint，可选）

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
- 首次启动自动执行迁移，并 seed 管理员 `admin / admin123`（仅当 users 表为空）。

### 3. 启动 Agent

```bash
# 反向模式（默认）
cargo run -p helm-agent -- --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token-change-me

# 正向模式
cargo run -p helm-agent -- --agent-id my-host --conn-mode forward --listen-addr 0.0.0.0:50052
```

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
| `just fmt` | `cargo fmt` |
| `just fmt-check` | `cargo fmt -- --check` |
| `just lint` | `cargo clippy --all-targets -- -D warnings` |
| `just test` | `cargo test` |
| `just buf-lint` | `buf lint` |
| `just buf-breaking` | `buf breaking --against '.git#ref=HEAD~1'` |
| `just db-up` / `db-down` | 启停 Postgres |
| `just run-server` / `run-agent` | 运行 server / agent |

## 端到端 smoke

```bash
./scripts/e2e-smoke.sh        # 一键：Postgres + Server + Agent → 登录 → 下发命令 → 验证 succeeded
./scripts/e2e-scheduler.sh    # 定时任务持久化恢复：建 schedule → 重启 Server → 验证恢复执行
```

两个脚本都 `set -euo pipefail`，退出时 `pkill target/debug/helm-*` 清理进程。

## 发布门禁

合并前须通过（见 `docs/plantree/baseline/test-and-release-gates.md`）：

1. `cargo fmt --check`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. `buf lint`（契约风格）+ `buf breaking --against '.git#ref=HEAD~1'`（契约兼容）
5. 无密钥/敏感信息进 diff

## 部署要点

- **生产必改**：`HELM_SERVER_TOKEN`、`HELM_JWT_SECRET` 两个默认值均为 `dev-*-change-me`。
- Agent 单二进制，Windows / Linux / macOS 各自 `cargo build --release -p helm-agent` 交叉产出（当前无交叉编译脚本，见 [current-state.md](current-state.md) 开放项）。
- 无 CI 配置文件（无 `.github/workflows`）；门禁当前为本地 `just check` + 手动 `buf` 检查。
- 反向模式 Agent 穿透 NAT；正向模式需 Server 能直达 Agent 的 `HELM_LISTEN_ADDR`。
