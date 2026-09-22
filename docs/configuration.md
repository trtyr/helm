# 配置参考（环境变量）

优先级：CLI 参数 > 环境变量 > 编译期烙入 > 内置兜底。

## Server

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_HTTP_ADDR` | `0.0.0.0:8080` | HTTP 监听地址 |
| `HELM_GRPC_ADDR` | `0.0.0.0:50051` | gRPC 监听地址（Agent 反向连入） |
| `HELM_DATABASE_URL` | `postgres://helm:helm@localhost:5433/helm` | Postgres 连接串 |
| `HELM_SERVER_TOKEN` | `dev-token-change-me` | Agent 认证 token（空则拒绝所有 Agent；生产必须改） |
| `HELM_SERVER_TOKENS` | 空 | 额外接受的 token（逗号分隔，多值轮换用：加新 → 全部 Agent 换新 → 删旧） |
| `HELM_JWT_SECRET` | `dev-secret-change-me` | JWT 签名密钥（生产必须改） |
| `HELM_BOOTSTRAP_ADMIN_USER` | `admin` | 初始管理员用户名（**仅当 users 表为空**、首次启动建号时生效；之后改名走控制台/API） |
| `HELM_BOOTSTRAP_ADMIN_PASSWORD` | `admin123` | 初始管理员口令（同上）。仍是出厂值且 `HELM_REQUIRE_STRONG_DEFAULTS=true` 时**拒绝启动**；否则建号打 ERROR——口令不落日志 |
| `HELM_REQUIRE_STRONG_DEFAULTS` | 关 | 出厂凭据守卫：置 true 后「server token / jwt secret / 初始管理员口令」仍是出厂值即拒绝启动（取值兼容 1/0/yes/no） |
| `HELM_MTLS` | 关 | mTLS（`--mtls`） |
| `HELM_TLS_DIR` | 空 | TLS 材料目录（持久化 CA，重启不换 CA；mTLS 部署强烈建议） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_WEB_DIST_DIR` | 空 | 托管控制台静态资源目录（前后端一体部署；有 dist 时 `GET /` 回 SPA，未匹配路径回 index.html） |
| `HELM_RETENTION_DAYS` | `90` | 数据保留天数：metrics / alerts / notifications / 已吊销 api_keys / jobs / audit_logs / file_transfers / status_events |
| `HELM_IR_SNAPSHOT_KEEP_PER_AGENT` | `20` | IR 快照每主机保留最近 N 条（0 = 不清理） |
| `HELM_IR_PAGE_CACHE_TTL_DAYS` | `30` | IR 页面缓存超 N 天未刷新即清（0 = 不清理） |
| `HELM_HEARTBEAT_TIMEOUT` | `30` | 心跳超时阈值（秒） |
| `HELM_OFFLINE_ALERT_MINS` | `30` | 持续离线超该阈值升级为告警（0 = 禁用） |
| `HELM_JOB_TIMEOUT_SECS` | `300` | running Job 超时兜底（0 = 禁用） |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_LOG_DIR` | 空 | 日志目录（非空按天滚动落文件，保留天数同 `HELM_RETENTION_DAYS`） |
| `HELM_SESSION_IDLE_TIMEOUT` | `300` | 会话空闲超时（秒） |

### 离线签发与 Agent 现场编译

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_ISSUE_CERT` | 关 | 离线签发 agent 证书三件套后退出（forward 预置分发用） |
| `HELM_ISSUE_AGENT_ID` / `HELM_ISSUE_SAN` / `HELM_ISSUE_OUT_DIR` | 空 | issue-cert 参数：agent 标识 / SAN 列表 / 输出目录 |
| `HELM_AGENT_SOURCE_DIR` | `.` | Agent 源码工作区（「生成 Agent」现场编译用；须能执行 cargo） |
| `HELM_CROSS_TOOLS_DIR` | 未设 | musl 交叉工具链目录（缺省 `<源码工作区>/.cargo-musl/bin`） |

## Agent

| 变量 | 默认 | 说明 |
|------|------|------|
| `HELM_AGENT_ID` | 空 | 唯一标识（缺省取编译期烙入值，再退回主机名） |
| `HELM_SERVER_ADDR` | `http://127.0.0.1:50051` | Server gRPC 地址（可编译期烙入） |
| `HELM_AGENT_TOKEN` | 空 | 注册 token |
| `HELM_CONN_MODE` | `reverse` | reverse（主动连 Server）/ forward（Agent 监听，Server 拨号） |
| `HELM_LISTEN_ADDR` | `0.0.0.0:50052` | forward 模式监听地址 |
| `HELM_CERT_DIR` | 空 | 证书缓存目录（非空启用 mTLS） |
| `HELM_SERVER_HTTP_ADDR` | 空 | 换证书用的 HTTP 地址（缺省按 gRPC 地址同 host + 8080 推导；控制台在 443 时必须显式设置——**最常见的坑**） |
| `HELM_TLS_SERVER_NAME` | `localhost` | mTLS server 证书 SAN 名 |
| `HELM_LOG` | `info` | 日志级别 |
| `HELM_LOG_DIR` | 空 | 日志目录（非空按天滚动落文件） |

### 编译期烙入（「生成 Agent」现场编译注入）

`HELM_BAKE_SERVER_ADDR` / `HELM_BAKE_AGENT_TOKEN` / `HELM_BAKE_AGENT_ID` / `HELM_BAKE_CONN_MODE` / `HELM_BAKE_LISTEN_ADDR`（`agent/build.rs` 白名单）。`HELM_BAKE_AGENT_ID` 不烙入时，目标机首跑按主机名自动生成——一份二进制通吃同平台主机。

## 生产部署

三处必改的密钥（`openssl rand -hex 32`）、上线前安全清单、备份与恢复，见 [deploy/README.md](../deploy/README.md)。
