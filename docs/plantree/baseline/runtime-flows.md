# 运行时流程

Role: detail-shard（baseline 全局上下文）
Status: active

> 已实现链路，与 [docs/architecture.md](../../architecture.md) 一致。「已实现 / 未实现」以当前代码为准。

## 1. 反向连接与注册（默认模式，Agent 主动连 Server）

```text
Agent 启动 → 读配置 → 拨号 Server 端点（默认明文 gRPC，`--mtls` 时走 TLS 双向认证）
  → 建立 bidirectional stream（一条流承载双向信令）
  → 发送 Register(agent_id, token, host_info)
  → Server 校验 token（严格匹配，空则拒绝）→ 按 hostname 复用/新建 host + upsert agent
  → 注册到 connection_registry（活跃连接表）→ 回 RegisterAck
  → Agent 进入心跳循环（Heartbeat 每 10s）+ 监控循环（30s）
```

## 2. 命令下发与执行（反向模式）

```text
控制台 → HTTP POST /api/v1/exec
  → ExecService 创建 Job（状态 = queued）→ 经 registry 推送 ExecRequest
  → Agent 执行 → 流式回报 stdout/stderr 分块 + finished 结果
  → Server 落库 Job 终态（succeeded/failed）→ 前端轮询结果或 `jobs/{id}/stream` 实时流
```

## 3. 状态上报

```text
Agent 定时（30s）采集 CPU / 内存 / 磁盘 / 网络 / 进程数
  → 流上推送 MetricReport
  → Server 逐条落库（时间序列）→ 前端看板
```

## 4. 文件传输

```text
控制台发起（上传/下载）
  → Server 创建 FileTransfer 记录
  → 流上下发 FileRequest（方向、路径、大小、分块策略）
  → 分块传输 → sha256 校验和 → 完成落库
```

## 5. 正向连接（同区域内网，Server 主动连 Agent）

```text
Agent 以 forward 模式监听 gRPC 端口（ForwardAgentService）
  → Server 按需（POST /api/v1/forward/exec）作为 gRPC client 拨号 Agent
  → 建立双向流，后续信令与反向模式同构
```

## 关键不变量

- 一条双向流是**单一事实来源**：Agent 在线 = 存在活跃流。（已实现）
- 断线重连后重新 Register（已实现）；**未完成 Job 结果的重放与超时兜底未实现**（Agent 中途断开时 Job 停留 running）。
- Job/FileTransfer 有稳定 ID；**重复上报去重未实现**（当前按最后一条覆盖）。
