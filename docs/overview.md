# Overview — helm 集中式运维平台

## 一句话

一个**集中式运维平台后端**：把轻量 Agent 下发到目标主机（Windows / Linux / macOS），即可从控制端对整批主机做**远程命令执行、文件上下传、指标采集、定时任务**。

## 两大组件

```text
[控制台 / HTTP API] ──► Server 控制端 ◄──gRPC 双向流──► Agent 被控端（各目标主机）
```

- **Server（`server/`）**：集中管理、任务编排、状态收集、持久化（Postgres）、对外 HTTP API。
- **Agent（`agent/`）**：装在目标机，执行命令、采集指标、文件传输。单二进制、跨平台。
- **proto（`proto/`）**：gRPC 契约（protobuf），Server 与 Agent 的**单一事实来源**。

## 核心能力

| 能力 | 说明 |
|------|------|
| 命令执行 | 控制台下发命令 → Server 建 Job → 经双向流推送 → Agent 执行并流式回报 stdout/stderr + 退出码 |
| 文件传输 | upload（下发到目标机）/ download（从目标机取回），分块 + sha256 校验和 |
| 指标采集 | Agent 每 30s 采集 CPU / 内存 / 进程数，上报落库（时间序列） |
| 定时任务 | 按间隔周期性下发命令，任务落库，**Server 重启后自动恢复调度** |
| 正向连接 | 同区域内网时，Agent 监听、Server 主动拨号执行 |

## 连接模式

- **反向（默认）**：Agent 主动连 Server，穿透 NAT/防火墙。
- **正向**：Agent 监听 gRPC 端口，Server 主动拨号（同区域内网）。

两种模式复用**同一套信令协议**（gRPC 双向流 `OpenChannel` / `OpenForwardChannel`），
见 [architecture.md](architecture.md) 与 [api.md](api.md)。

## 技术轮廓

- **语言/框架**：Rust（edition 2024），tokio 异步；axum（HTTP）+ tonic（gRPC）+ sqlx（Postgres）。
- **仓库形态**：Cargo workspace，三个成员 crate：`proto` / `server` / `agent`。
- **持久化**：PostgreSQL，sqlx 编译期嵌入迁移。
- **无前端**：控制台是独立工程，不在此仓库内；本仓库只暴露 HTTP API 供其消费。

详见 [tech-stack.md](tech-stack.md)。
