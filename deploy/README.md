# helm 生产部署与运维（单机云主机）

> 本目录是 **生产部署制品**。仓库根目录的 `docker-compose.yml` 是**开发用**
> （弱默认凭据、端口直挂宿主、无 Caddy），两份互不影响。
> 事实口径（配置项全表、保留策略、门禁）见 engram《部署与运维》《测试与门禁》；
> 本文只回答「怎么部署、怎么运维」。

## 0. 拓扑

```text
                  ┌──────────────── 云主机 ────────────────┐
 浏览器 ─443/TLS─▶ caddy ─/api /mcp─▶ server:8080
                  │   └─其余─▶ console/dist（静态 + SPA fallback）
                  │                  server ─▶ postgres:5432（仅内网）
 Agent ──50051───▶ server（gRPC；建议 mTLS）
                  └───────────────────────────────────────┘
```

- **Caddy** 终结 TLS（自动申请证书）、托管控制台静态资源、反代 API/MCP
  （含 WS 实时流）。
- **server** 对外只暴露 gRPC `50051`；HTTP 只绑宿主回环供 Caddy 使用。
- **postgres** 不发布到宿主。
- Agent **直连 50051**（保留 mTLS 能力）；若必须统一走 443，
  见 `prod/Caddyfile` 末尾注释。

## 1. 首次部署

```bash
git clone <repo> helm && cd helm

# ① 密钥（3 处必改）
cd deploy/prod && cp env.example .env && vi .env
#   POSTGRES_PASSWORD / HELM_SERVER_TOKEN / HELM_JWT_SECRET ← openssl rand -hex 32
#   保持 HELM_REQUIRE_STRONG_DEFAULTS=true：忘改就起不来（取值推荐 true，兼容 1/0/yes/no）
#   （默认只打 ERROR 日志、不拦，见 engram《部署与运维》§8.1）

# ② 控制台静态资源（Caddy 直接托管，不打进 server 镜像）
pnpm --dir ../../console install --frozen-lockfile
pnpm --dir ../../console build      # 产出 ../../console/dist

# ③ 域名（在 .env 里改，不必动 Caddyfile）
#   HELM_SITE_ADDR=ops.example.com ← 换成你的域名（Caddy 据此自动申请证书）

# ④ 起
docker compose up -d --build
docker compose ps                   # 三个服务都 healthy/running

# ⑤ 验收
curl -fsS https://ops.example.com/healthz
docker compose logs -f server | head -30   # 不应出现 insecure default 告警

# 本地预演（想先在本机跑通再上云，且不碰 Let's Encrypt 限额）：
#   .env 里 HELM_SITE_ADDR=localhost → Caddy 用内部 CA 自签，加 -k 跳过校验
#   curl -k https://localhost/healthz   → 200
#   curl -k https://localhost/          → 控制台 index.html
```

**⑥ 改掉默认管理员**：首次启动会 seed `admin / admin123`（日志里是 ERROR 级）。
登录控制台 → 设置 → 账号 → 改密码；或调 `POST /api/v1/auth/change-password`。
这一步不做等于门开着——A1 守卫只报错，不替你做。

## 2. 接入 Agent

### 反向模式（默认，Agent 主动连）

- Windows：
  `deploy/install-windows-service.ps1 -ServerAddr http://<host>:50051 -Token <token>`
  （需先装 nssm，管理员 PowerShell；脚本默认 token 是出厂值，**必须显式传参**）
- Linux：用 `deploy/helm-agent.service`，替换 `${HELM_AGENT_ID}` /
  `${HELM_SERVER_ADDR}` / `${HELM_AGENT_TOKEN}` 三个占位符。

> ⚠ **最常见的坑**：Agent 换证书走的是 HTTP 端点 `/api/v1/agents/cert`，
> 其地址默认由 `server_addr` 推导成 `http://<host>:8080`。生产把控制台放在 443
> 时，必须显式设 `HELM_SERVER_HTTP_ADDR=https://ops.example.com`，
> 否则换证书那一步会打到 8080 上。

### mTLS 证书的两条路径

- 反向：Agent 首次启动用 token 走 `POST /api/v1/agents/cert` 换证书 → 之后走 mTLS
  （服务端要求 CSR 主体 CN 等于 `agent_id`，不匹配或缺 CN 一律 403）。
- 正向：`helm-server --issue-cert --issue-agent-id <id> --issue-san <域名或IP>
  --issue-out-dir <dir>` 离线签发三件套（cert/key/ca）后**退出不启服务**，
  再把文件预置到目标机。

## 3. 日常运维

### 重启会发生什么（务必知道）

**T006 起服务端有优雅停机**：SIGTERM（`docker compose stop/restart`、`systemctl stop`）会
停接新连接 → 排空在飞请求 → 退出（退出码 0；长连接最多等 15s 兜底后被切断）。

| 影响面 | 后果 |
|---|---|
| 在飞 job | 优雅停机**不会**凭空完成命令：启动时对账（T007）把上一进程遗留的 `running`/`queued` 置为 `failed`，`output` 写明「服务重启前未完成（非超时）」——不再等 300s 被误标 `timed_out`（**假超时已消除**） |
| 在飞文件传输 | 同一轮启动对账 → `failed`（不再挂到 600s 才失败） |
| 终端 / SOCKS 代理 | 随排空断开（会话本就不可跨进程续） |
| 主机上下线 | 刷一波 offline → Agent 3s 起指数退避重连（封顶 300s），`status_events` 记录这一波 |
| listeners / 定时任务 | 从库恢复，无损 |

→ 重启仍建议安排在低峰（在飞 job 会被判失败、需重发），但**不会再有「假超时」**，
也不会留下「一直挂着、不知道要等到什么时候」的状态。

### 改 `POSTGRES_PASSWORD` 之后（易踩坑）

`POSTGRES_PASSWORD` 只在**数据卷首次初始化**时生效。卷已存在时改 `.env` 里的值不会改
数据库内部的口令，server 会以 `password authentication failed for user "helm"` 反复重启
（本机实跑时踩过一次）。出路二选一：在库里改口令
（`docker compose exec postgres psql -U helm -c "ALTER USER helm PASSWORD '新值'"`），
或接受丢数据重建卷：`docker compose down -v && docker compose up -d`。

### 备份 / 恢复

```bash
cd deploy/prod
./backup.sh                 # pg_dump | gzip → ./data/backups/
./backup.sh --keep 30
./backup.sh --restore ./data/backups/helm-<ts>.sql.gz
```

### 日志与容量

- server 日志：`./data/logs`（按天轮转，保留 `HELM_RETENTION_DAYS` 天）。
- 后台清理（每 24h 一轮）：metrics / alerts / notifications / jobs / audit_logs /
  file_transfers / **status_events** 超 `HELM_RETENTION_DAYS`（默认 90 天）删。
  status_events（「谁什么时候上下线」的时序账）自 T008 起纳入——它的增长与
  主机数 × 上下线频次成正比，云上磁盘要钱。
- **IR 取证表有独立保留策略**（T009，两项都可配，0 = 不清理）：
  - `ir_snapshots`：**每主机保留最近 `HELM_IR_SNAPSHOT_KEEP_PER_AGENT` 条**（默认 20）。
    快照是取证资产（基线对比/差异取证），按「每主机条数」封顶而非按时间一刀切——
    避免把唯一一份基线也删掉。
  - `ir_page_cache`：**超 `HELM_IR_PAGE_CACHE_TTL_DAYS` 天未刷新即清**（默认 30）。
    该表以 `(agent_id, kind)` 为主键、写入即刷新 `created_at`，行数本身有界；
    这里治的是陈旧内容与「已注销主机留下的死缓存」。
  - **磁盘预算算法**：占用 ≈ `Σ主机(快照数 × 单快照体积)` + `主机数 × kind 数 × 单缓存体积`。
    单快照体积由 findings 条目数决定。上云前用现网实测校准一次：

    ```bash
    docker compose exec postgres psql -U helm -d helm -c "
      SELECT (SELECT count(*) FROM ir_snapshots) AS snapshot_rows,
             pg_size_pretty(pg_total_relation_size('ir_snapshots')) AS snapshots,
             pg_size_pretty(pg_total_relation_size('ir_page_cache')) AS page_cache;"
    ```

    量级参考：单快照 findings 数千条 ≈ 数百 KB~数 MB；按默认 20 条/主机，
    百台主机≈数 GB——按实测结果调 `HELM_IR_SNAPSHOT_KEEP_PER_AGENT`。

```bash
docker compose exec postgres psql -U helm -d helm \
  -c "SELECT count(*) FROM status_events;"
```

### 升级

- server：`git pull && cd deploy/prod && docker compose up -d --build`
  （Caddy / DB 不动）。
- 控制台：`pnpm --dir console build`（Caddy 直接读新产物，无需重启 server）。
- Agent：**手工**替换二进制并重启服务（当前无自动升级通道）。
- **改 Rust 版本时**：`Dockerfile` 的构建阶段与运行阶段必须保持**同一个 Debian 代号**
  （现均为 bookworm）。混用会出现 `GLIBC_2.39 not found`、容器无限重启——2026-09-21
  冒烟测试实测过一次（当时构建阶段是基于 trixie 的 `rust:1.97-slim`，运行阶段是 bookworm）。

## 4. 上线前安全清单（逐条勾）

| # | 事项 | 落点 |
|---|---|---|
| 1 | TLS 终结（443 + 自动证书） | `prod/Caddyfile` |
| 2 | 三个强随机值已改 | `prod/.env` |
| 3 | `HELM_REQUIRE_STRONG_DEFAULTS=true`（弱值拒启；取值推荐 true，兼容 1/0/yes/no） | `prod/.env` |
| 4 | 默认管理员口令已改 | 控制台 → 设置 → 账号 |
| 5 | DB 不对外（内网 + 无 ports 发布） | `prod/docker-compose.yml` |
| 6 | Agent token 非出厂值 | `install-windows-service.ps1 -Token` |
| 7 | DB 备份 + 恢复演练 | `prod/backup.sh` |
| 8 | 自动重启策略 + healthcheck | compose 三服务 |
| 9 | 容量清理策略已定：`status_events` 纳入 `HELM_RETENTION_DAYS`；IR 表按独立策略 | 见 §3 |
| 10 | 安全组仅放 443 + 50051（50051 限源 IP 或开 mTLS） | 云控制台 |

## 5. 已知限制（现状说明，非缺陷申报）

- **单节点**：连接状态在进程内存里，重启期间全部 Agent 断线重连（无跨实例共享）；
  Postgres 单实例、无 HA。横向扩展需状态外置，目前不做。
- **单用户产品**：无多租户 / RBAC（产品定性如此），控制台账号只有一个。
- **优雅停机已具备（T006）**：`docker compose stop` 会走排空路径并退出码 0；重启语义与在飞
  job 的处置见 §3 的重启表。
- **agent-gen（现场编译 Agent）**：容器内可出 Linux 包——2026-09-21 实测
  `cargo build --release -p helm-agent` 用 **3m16s** 产出 **13MB** 二进制，吃的就是镜像里
  预置的依赖缓存；Windows 目标需宿主机带 mingw 工具链并注入 `HELM_CROSS_TOOLS_DIR`。
  缓存写在容器可写层，容器重建后仍在；若想让缓存跨**镜像重建**保留，可把宿主机目录
  挂到 `/workspace/target`（代价：挂载会盖掉镜像内的预置缓存，首次出包要重编全部依赖）。
- **无 Agent 自动升级通道**：升级靠手工替换二进制。
