# 本地开发指南

## 跑起来（开发模式）

```bash
git clone https://github.com/trtyr/helm && cd helm
docker compose up -d postgres            # 开发库，127.0.0.1:5433（helm/helm）
pnpm --dir console install && pnpm --dir console build
cargo run -p helm-server                 # 控制台 http://localhost:8080 · gRPC :50051
```

首次启动自动执行迁移并按 `HELM_BOOTSTRAP_ADMIN_USER / PASSWORD` 创建管理员（缺省 `admin / admin123`，打 ERROR 提醒）。

本地起一个 Agent：

```bash
cargo run -p helm-agent -- \
  --agent-id my-host --server-addr http://127.0.0.1:50051 --token dev-token-change-me
```

其他形态：

```bash
# 正向模式（Agent 监听，Server 拨号）
cargo run -p helm-agent -- --conn-mode forward --listen-addr 0.0.0.0:50052

# mTLS（Server 先 --mtls --tls-dir /tmp/helm-tls，Agent 带证书目录）
cargo run -p helm-agent -- --cert-dir /tmp/agent-cert
```

## 验证全链路

```bash
TOKEN=$(curl -s -X POST http://127.0.0.1:8080/api/v1/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin123"}' | jq -r .token)

curl -s -X POST http://127.0.0.1:8080/api/v1/exec \
  -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d '{"agent_id":"my-host","command":"uname","args":["-a"]}'
```

## 门禁

```bash
just check        # fmt + clippy(-D warnings) + test(真 Postgres) + windows 交叉编译 + console 四件套
just test         # 只跑测试；集成测试需要 5433 的 Postgres
just buf-lint     # protobuf 契约 lint
just console-check # 前端四件套：tsc -b / oxlint / vitest / vite build
```

- 测试约 220 条（`#[test]` / `#[tokio::test]` 属性计数）；集成测试在 `server/tests/`（16 个文件），连真实 Postgres、每例独立建库（`helm_itest`），不碰开发库
- Windows 侧代码在 macOS/Linux 上有两条编译验证通道：本地 mingw（`just windows-check`）与 CI 的 `windows` job；Windows-only 的纯函数已抽到平台无关模块（`ir/text`、`ir/regcodec`、`netfmt`）并在任意平台参与测试

## 端到端脚本

`scripts/` 下 10 个 e2e（自起 Postgres + Server，部分含 Agent）+ 3 个校验脚本：

```bash
python3 scripts/e2e-smoke.py       # 一键端到端 smoke
python3 scripts/e2e-phase8.py      # CRUD + 实时流
python3 scripts/e2e-phase9.py      # 通知中心
python3 scripts/e2e-mcp.py         # MCP：签发→握手→scope 裁剪→透传→越权→吊销
python3 scripts/check_openapi.py   # OpenAPI 契约与路由一致性（72 端点）
python3 scripts/check_docs.py      # 文档现状陈述与代码一致性
```

## 目录结构

```text
Cargo.toml            # workspace（proto / server / agent）
proto/                # 共享 protobuf 契约（gRPC 底座）
server/               # Server 控制端（axum + tonic + sqlx）
  src/domain          #   领域层（实体、状态机）
  src/application     #   应用层（用例编排）
  src/grpc            #   gRPC 适配层（Agent 连入）
  src/http            #   HTTP API 适配层（控制台）
  src/store           #   持久化层（sqlx + Postgres）
  migrations/         #   数据库迁移
agent/                # Agent 被控端（tokio，跨平台单二进制）
  src/ir/             #   IR 应急响应模块（16 个能力模块，Windows）
console/              # 前端控制台（Vite + React + xterm.js）
deploy/               # 部署制品（compose / Caddy / 备份 / systemd / nssm 脚本）
scripts/              # e2e 与校验脚本（Python）
docs/                 # openapi.yaml（机器契约）+ 本文档
```

## Windows 侧的原生能力说明

Agent 的 Windows 监控数据不依赖外部命令：服务走 SCM、网络适配器/连接表走 IpHelper API、磁盘走 GetLogicalDrives、进程路径走 QueryFullProcessImageName——无编码问题，采集口径稳定。

## API

完整契约见 [openapi.yaml](openapi.yaml)（OpenAPI 3.0.3，72 端点，`check_openapi.py` 保证与路由强一致）。认证统一 `Authorization: Bearer <JWT 或 helm_ 前缀 API key>`；WebSocket 走 `?token=`。
