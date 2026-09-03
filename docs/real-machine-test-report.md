# Linux 真机测试报告 — forward 模式全量验证

日期：2026-09-03 · 提交：`66aa763`（forward 持久连接） · 结果：**15/15 项通过**

## 1. 测试环境与拓扑

| 角色 | 机器 | 说明 |
|------|------|------|
| Server（控制端） | Mac（demotestdemacbook-air） | debug 二进制，HTTP `127.0.0.1:18081`，gRPC `0.0.0.0:50051`，Postgres（docker，5433） |
| Agent（被控端） | tencent-sg（VM-8-5-opencloudos，OpenCloudOS 9.6，x86_64） | musl 静态二进制，**forward 模式**监听 `0.0.0.0:50052`，公网 `43.163.80.102:50052` |

拓扑决策（用户确认）：tencent-sg 是公网机器，**只做被控端监听，Server 主动连它（forward）**；
agent 不反向回连控制端（tailscale DERP 路径 TCP 不通，且不符合安全边界）。Windows 侧停用清理、不测。

## 2. 前置开发：forward 模式持久连接

**问题**：原 forward 实现只有 `POST /api/v1/forward/exec` 一个入口（按需拨号、同步往返）。
文件/交互终端/服务管理/进程/网络端点全部走 `ConnectionRegistry`（反向持久连接），forward 主机无法使用。

**方案**（commit `66aa763`）：

- **协议同构**：forward 流上复用 reverse 的信令——连接建立后 agent 先发 `Register`，随后心跳（10s）+ 指标（30s）。
- **Server 侧 `ForwardManager`**：周期（10s）对照 `hosts` 表（`conn_mode='forward' AND addr<>''`）差分启停拨号循环；
  断线自动重连；`Register` 校验 token 后经 `AgentRepo::register_under_host` 挂到既定 host，并注册进
  `ConnectionRegistry` ——**此后全部控制端点（agent_id 路由）对 forward 主机原生可用，HTTP 层零改动**。
- **`InboundCtx` 抽取**：reverse（`agent_service.rs`）与 forward（`forward_manager.rs`）的入站消息处理收敛到
  `grpc/inbound.rs`，消除双份维护。
- 测试：`diff_hosts` 纯函数 4 个单测；reverse 回归（e2e-phase5/6/7/8 + 51 单测）全绿。

## 3. 测试结果矩阵（15/15 通过）

复现：`HELM_TEST_AGENT=linux-fwd8 HELM_TEST_PLATFORM=linux HELM_E2E_HTTP_ADDR=127.0.0.1:18081 python3 scripts/real-machine-test.py`

| # | 能力 | 结果 | 证据 |
|---|------|------|------|
| 1 | 在线状态 | ✅ | forward host `online=true`（reconciler 拨号 → Register → ConnectionRegistry） |
| 2 | 命令执行 | ✅ | `POST /exec` → job `succeeded` exit=0 |
| 3 | 文件上传 | ✅ | sha256 checksum_ok=true（Mac → 新加坡公网分块传输 + 64KiB 分块写盘） |
| 4 | 文件下载 | ✅ | 内容逐字节一致，checksum_ok=true |
| 5 | 列目录 | ✅ | `POST /files/list` 返回 18 项（Linux 权限位正常） |
| 6 | 进程列表 | ✅ | 410 个真实进程（sysinfo） |
| 7 | 进程 kill | ✅ | 不存在 pid 返回 `{ok:false}`（HTTP 200 语义正确） |
| 8 | 网络信息 | ✅ | `hostname=VM-8-5-opencloudos`（真实主机名）+ 接口地址 |
| 9 | 服务管理 | ✅ | start→`running`→日志增量落库→stop 全链路 |
| 10 | 监听器启停 | ✅ | 动态监听器 create/start/stop/delete |
| 11 | 审计落库 | ✅ | `GET /audit` 50 条记录（login/exec/文件/主机/监听器） |
| 12 | 监控指标 | ✅ | 66 条指标（cpu.usage / mem.* / **disk.usage** / net.* / proc.count） |
| 13 | 交互终端 | ✅ | WS + PTY，`echo PTY_REAL_MARKER` 回显 30 字节（Linux pty） |
| 14 | job 实时流 | ✅ | `WS /jobs/{id}/stream` 收到输出增量 |
| 15 | metrics 实时流 | ✅ | `WS /metrics/stream` 收到真实上报 `{"name":"cpu.usage","value":2.09}` |

## 4. 过程中发现并修复的问题

| 问题 | 根因 | 修复 |
|------|------|------|
| musl 交叉编译失败（Phase 7 起回归） | `reqwest` 默认 `native-tls`（openssl-sys）在 musl 交叉编译下不可构建 | 改 `rustls-tls`（`66aa763`） |
| reconciler 拨号循环无限重启 | diff 用空串占位 running addr，每轮都判成「addr 变更」 | tasks 记录真实 addr（`66aa763`） |
| e2e-phase5 「host 未显示 online」 | Phase 8 分页后 `GET /hosts` 默认 20 条，目标 host 挤出第一页 | e2e 查询显式 `limit=1000`（`66aa763`） |
| `pkill -f helm-agent` 导致 SSH 会话 exit 255 | 模式匹配到 host_exec 自身 bash 命令行，杀掉自己 | 用精确模式/kill PID |
| 公网传输 40KB/s 且抖动 | 腾讯云公网限速 | gzip + 512KB 分块 + 每块重试（11.9MB 约 8 分钟） |

## 5. 限制与后续项

1. **forward mTLS 未测**：mTLS 证书自动签发需 agent → Server HTTP（POST /agents/cert），与「agent 不回连」边界冲突；
   forward 链路的 TLS（agent 侧 ServerTlsConfig + Server 侧 ClientTlsConfig）是独立开发项，列入后续。
2. **真机上未执行 uninstall**：SelfDestruct 会删除真机二进制（测试环境需保留 agent），该能力由 e2e-phase5 覆盖。
3. **Windows 观察**（早前探索）：中文 locale 的 GBK 输出经 lossy UTF-8 转换会乱码（`ver` 输出「版本」→ 乱码），
   跨平台编码处理列为后续改进；Windows 侧已按要求清理（无进程/任务/文件）。
4. 测试期间 DB 清理：25 行 Phase 4/8 残留 forward host 行已删（reconciler 噪声源）。
