---
name: helm
description: helm 集中式运维平台操作技能。用平台 API key 通过 HTTP API 操作除 API key 管理之外的所有平台能力：主机/Agent 管理、远程命令执行、文件上下传、交互终端、常驻服务、进程/网络管控、指标与告警、通知、监听器、正向连接、审计。适用于用户要求查询/操作 helm 运维平台、远程管理主机、下发命令或脚本、查指标告警通知等场景。
---

# helm 运维平台操作

通过 helm Server 的 HTTP API 远程运维一批主机。本 skill 的脚本用 **API key**（或 JWT）
认证，可操作平台上除 api-key 管理之外的全部能力（api-keys 端点仅收 JWT——key 不可自管，
这是平台设计；管理 key 请走控制台或用 JWT 直接调 `POST/DELETE /api/v1/api-keys`）。

## 获取 / 更新本包（API key 即取即用）

本包由 helm Server 内嵌分发（决策 011），持有任意有效凭据即可随时下载或校验版本：

```bash
# 下载完整包（zip），解压即用，放置位置随意
curl -H "Authorization: Bearer $HELM_TOKEN" "$HELM_URL/api/v1/skill" -o helm-skill.zip

# 查看清单（版本 + 文件列表 + sha256），用于升级对比
curl -H "Authorization: Bearer $HELM_TOKEN" "$HELM_URL/api/v1/skill/manifest"
```

两个端点 API key 与 JWT 均可——否则会出现「先有 key 还是先有 skill」的死锁。

## 连接配置（先确认，再调脚本）

脚本零第三方依赖（Python 3 标准库）。按优先级从环境变量取配置：

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_URL` | `http://127.0.0.1:8080` | Server HTTP 地址 |
| `HELM_TOKEN` | — | **推荐**。`helm_` 前缀 API key 或 JWT |
| `HELM_USERNAME` / `HELM_PASSWORD` | `admin` / `admin123` | 无 token 时自动登录（开发默认值） |

无 `HELM_TOKEN` 时脚本会用用户名密码登录并缓存 JWT（临时目录，24h 内复用）。
开始工作前先跑一次 `python <skill>/scripts/auth.py status` 确认 Server 可达与凭据来源。

所有脚本支持 `--url` 临时覆盖地址；输出一律 JSON（`--raw` 时为简洁文本）。

## 脚本索引（scripts/，每个功能域一个）

| 脚本 | 域 | 常用命令 |
|------|----|---------|
| `auth.py` | 认证自检 | `status`、`login` |
| `hosts.py` | 主机 CRUD/标签 | `list [--tag x --online]`、`create/update/delete`、`set-tags` |
| `agents.py` | Agent 生命周期 | `list`、`get`、`tags`、`deregister`、`uninstall`（危险） |
| `exec.py` | 命令执行/Job | `run <agent> -- uname -a`、`submit`、`jobs`、`job`、`wait` |
| `tasks.py` | 脚本/定时任务 | `script <agent> --sh "..." / --ps "..."`、`schedule --interval 60 --sh ...` |
| `files.py` | 文件传输 | `upload/download`（**local_path 在 Server 侧**）、`ls` |
| `services.py` | 常驻服务 | `list/create/start/stop/restart/logs/tail/delete` |
| `processes.py` | 进程/网络 | `ps`、`kill --pid`、`net` |
| `metrics.py` | 指标/告警 | `get <host_uuid>`、`alerts` |
| `notifications.py` | 通知中心 | `list [--unread]`、`unread`、`read`、`read-all` |
| `listeners.py` | gRPC 监听器 | `list/create/update/start/stop/delete` |
| `forward.py` | 正向连接 | `exec --hostname x -- cmd ...` |
| `audit.py` | 审计 | `list` |
| `streams.py` | 实时流(WS)/终端 | `job <id>`、`metrics`、`notifications`、`service-logs <id>`、`terminal <agent>` |

依赖模块：`common.py`（配置/认证/HTTP）、`ws.py`（纯标准库 WebSocket 客户端）。

## 典型工作流

```bash
S=<skill目录>/scripts

# 1. 看有哪些主机、谁在线
python $S/hosts.py list --online

# 2. 在目标机执行命令（阻塞等结果，退出码=命令退出码）
python $S/exec.py run my-host -- uname -a
python $S/exec.py run my-host -- docker ps --format '{{.Names}}'

# 3. 跑一段脚本（bash / powershell 封装）
python $S/tasks.py script my-host --sh "df -h && free -m"

# 4. 传文件（注意：--local 是 Server 机器上的路径）
python $S/files.py upload my-host --local /srv/dist/app.tar.gz --remote /tmp/app.tar.gz

# 5. 看指标与告警（host_id 从 hosts.py list 拿）
python $S/metrics.py get <host_uuid>
python $S/metrics.py alerts

# 6. 常驻服务：创建 + 起停 + 日志
python $S/services.py create my-host --name web --command python --args "-m,http.server" --restart-policy always
python $S/services.py tail <service_uuid>

# 7. 通知与审计
python $S/notifications.py list --unread
python $S/audit.py list
```

注意事项：

- **本地选项写在 `--` 之前**：exec.py / forward.py 中 `--` 之后的参数原样传给目标机
  （`run --raw -- foo` 正确；`run agent -- foo --raw` 会把 `--raw` 传给远端）。
- `agent_id` 是 Agent 业务标识（`agents.py list` 的 `id` 字段，字符串）；`host_id` 是主机 UUID。
- exec 是异步 Job 模型：`run` 已封装等待；`submit --raw` 打印裸 job_id 便于管道；
  需要流式输出用 `streams.py job <job_id>`。
- **本机是 Git Bash/MSYS 时** `/` 开头参数会被路径改写（`/c`→`C:/`），优先用 `tasks.py`
  的 `--sh`/`--ps` 封装，详见 references/troubleshooting.md。
- **卸载/注销是破坏性操作**（`agents.py uninstall/deregister`），执行前必须向用户确认。
- Windows 目标机的命令输出按 OEM 代码页由 Server 解码；跨平台命令优先用 `tasks.py script`。

## 深入参考（references/）

- [api.md](references/api.md) — 全部端点速查：请求体/响应形状/分页/错误码
- [architecture.md](references/architecture.md) — 平台架构、连接模式（reverse/forward）、认证模型、数据模型
- [troubleshooting.md](references/troubleshooting.md) — 按现象排查（401/403/409、乱码、文件路径、挂起 job）

平台本体仓库：`D:\Code\Rust\helm`（契约 `docs/openapi.yaml`，文档 `docs/`，规划 `docs/plantree/`）。
脚本覆盖不到的平台能力（目前仅：定时任务的列出/删除、api-keys 管理）直接按 api.md 用 curl/HTTP 调。
