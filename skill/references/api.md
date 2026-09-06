# Helm 平台 API 参考（skill 视角）

平台契约全量在仓库 `docs/openapi.yaml`（OpenAPI 3.0.3，46 端点）；本文是给
Agent/脚本用的速查版，字段与响应形状均对齐后端实现。

## 通用约定

- Base：`{HELM_URL}/api/v1`；除 `/healthz`、`/auth/login`、`/agents/cert`、WS 端点外均需
  `Authorization: Bearer <JWT 或 helm_ 前缀 API key>`。
- WS 端点用 `?token=` 传同一凭据（握手无法带 header）。
- **api-keys 管理端点仅收 JWT**：API key 调用返回 403 `forbidden`（key 不可自管）。
- 分页：`?page=&limit=`（各端点默认不同，常见 20/50）。
- 错误统一：`{ "error": { "code": "...", "message": "..." } }`；
  code ∈ not_found / unauthorized / forbidden / invalid_argument / not_connected / storage / io / internal。
  状态映射：404 / 401 / 403 / 400 / 409(Agent 离线) / 500。

## 端点速查

### 主机 hosts
| 方法 路径 | 请求 | 响应 |
|---|---|---|
| GET `/hosts?tag=&page=&limit=` | — | `{hosts: [HostView]}`，含 `online/last_seen/stale` |
| POST `/hosts` | `{hostname, conn_mode?: reverse\|forward, addr?, tags?}` | `{host}` |
| PUT `/hosts/{id}` | `{hostname, ...同上, os?/arch?/platform?}` | `{host}` |
| DELETE `/hosts/{id}` | — | `{ok}`（软删） |
| POST `/hosts/{id}/tags` | `{tags: [string]}` | `{host}` |

### Agent agents
| GET `/agents` | — | `{agents: [{id, host_id, version, hostname, last_heartbeat_at}]}` |
|---|---|---|
| GET `/agents/{id}` | — | `{agent: {..., online}}` |
| DELETE `/agents/{id}` | — | `{ok}` 注销（孤儿主机软删） |
| PUT `/agents/{id}/tags` | `{tags}` | `{host}` |
| POST `/agents/{id}/uninstall` | `{remove_binary?: bool=true}` | `{ok}`（409=离线） |

### 命令执行 exec / jobs
| POST `/exec` | `{agent_id, command, args?, timeout_secs?, working_dir?}` | `{job_id}` |
|---|---|---|
| GET `/jobs?page=&limit=` | — | `{jobs: [Job]}` |
| GET `/jobs/{id}` | — | `{job}`：status ∈ queued/running/succeeded/failed/timed_out/cancelled，`output`、`exit_code` |
| GET `/jobs/{id}/stream?token=` | WS | 二进制帧 = 输出增量 |

Job 终态判定：有 error → failed；exit_code 0/None → succeeded；其余 → failed。

### 文件 files（**local_path 在 Server 侧**）
| POST `/files/upload` | `{agent_id, local_path, remote_path}` | `{transfer_id, checksum_ok}` |
|---|---|---|
| POST `/files/download` | `{agent_id, remote_path, local_path}` | `{transfer_id, checksum_ok}` |
| POST `/files/list` | `{agent_id, path}` | `{path, entries: [{name, is_dir, size, modified_unix_ms, mode}]}` |

### 任务 tasks
| POST `/tasks/script` | `{agent_id, command, args?}` | `{job_id}`（复用 exec） |
|---|---|---|
| POST `/tasks/schedule` | `{agent_id, command, args?, interval_secs}` | `{task_id}` |

定时任务重启自动恢复；无列出/删除端点（平台缺口）。

### 指标与告警 metrics
| GET `/metrics?host_id=&limit=` | — | `{metrics: [{host_id, name, value, labels, ts}]}` |
|---|---|---|
| GET `/alerts?page=&limit=` | — | `{alerts: [{id, host_id, metric_name, threshold, value, level, created_at}]}` |

阈值硬编码 cpu/mem/disk > 90%。指标由 Agent 每 30s 采集上报。

### 通知 notifications
| GET `/notifications?page=&limit=&unread=` | — | `{notifications: [{id, host_id, kind, message, read, created_at}]}` |
|---|---|---|
| GET `/notifications/unread-count` | — | `{count}` |
| POST `/notifications/{id}/read` | — | `{ok}`（404=不存在） |
| POST `/notifications/read-all` | — | `{updated}` |

kind ∈ online / offline / alert；同 host 同 kind 5 分钟冷却合并。

### 常驻服务 services
| GET `/services?page=&limit=` | — | `{services: [Service]}` |
|---|---|---|
| POST `/services` | `{agent_id, name, command, args?, restart_policy?: "no"\|"always"}` | `{service}` |
| PUT `/services/{id}` | `{name, command, args?, restart_policy?}` | `{service}` |
| DELETE `/services/{id}` | — | `{ok}`（running 需先 stop） |
| POST `/services/{id}/start|stop|restart` | — | `{ok}` |
| GET `/services/{id}/logs` | — | `{log}`（快照） |
| GET `/services/{id}/logs/stream?token=` | WS | 二进制帧 = 日志增量 |

### 进程/网络 processes
| POST `/processes/list` | `{agent_id}` | `{processes: [{pid, name, cpu_percent, mem_bytes}]}` |
|---|---|---|
| POST `/processes/kill` | `{agent_id, pid}` | `{ok}`（pid 不存在 ok=false 仍 200） |
| POST `/net/info` | `{agent_id}` | `{hostname, interfaces: [{name, addrs}]}` |

### 监听器 listeners
| GET `/listeners` | — | `{listeners: [{id, name, addr, proto, status, ...}]}` |
|---|---|---|
| POST `/listeners` | `{name, addr, proto?: grpc, auth?}` | `{listener}`（默认 stopped） |
| PUT `/listeners/{id}` | `{name, addr, proto?, auth?}` | `{listener}` |
| DELETE `/listeners/{id}` | — | `{ok}` |
| POST `/listeners/{id}/start|stop` | — | `{ok}` |

### 正向连接 forward
| POST `/forward/exec` | `{hostname? | agent_addr?, command, args?}` | `{output, exit_code}`（hostname 需 forward 主机且已配 addr） |
|---|---|---|

### 审计 audit
| GET `/audit?page=&limit=` | — | `{audit: [{id, actor, action, resource, detail, created_at}]}` |
|---|---|---|

### 实时流（WS，`?token=`）
| GET `/agents/{id}/terminal?token=&cols=&rows=` | 交互 PTY；客户端帧首字节 0x01=输入、0x02=resize(JSON `[cols,rows]`)；服务端二进制=输出 |
|---|---|
| GET `/jobs/{id}/stream?token=` | job 输出增量 |
| GET `/services/{id}/logs/stream?token=` | 服务日志增量 |
| GET `/metrics/stream?token=` | JSON 指标点 |
| GET `/notifications/stream?token=` | JSON 通知 |

### 认证 auth / api-keys（参考，key 不可调）
| POST `/auth/login` | `{username, password}` | `{token}`（JWT，24h） |
|---|---|---|
| GET/POST `/api-keys`、GET/DELETE `/api-keys/{id}` | — | **仅 JWT**；创建响应含一次性明文 key |

### Skill 包分发（决策 011）
| GET `/skill` | — | zip 完整包（`Content-Disposition: attachment`）；JWT / API key 均可 |
|---|---|---|
| GET `/skill/manifest` | — | `{name, version, files: [{path, size, sha256}]}`（与 zip 内容一致） |

### Agent 证书 cert（Agent 用，脚本一般不碰）
| POST `/agents/cert` | `{agent_id, token, csr_pem}` | `{cert_pem, ca_cert_pem}` |
|---|---|---|
