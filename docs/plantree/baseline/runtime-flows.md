# 运行时流程

Role: detail-shard（baseline 全局上下文）
Status: planning

## 1. 反向连接与注册（默认模式，Agent 主动连 Server）

```text
Agent 启动 → 读配置 → TLS 拨号 Server gRPC 端点
  → 建立 bidirectional stream（一条流承载双向信令）
  → 发送 Register(agent_id, token, host_info)
  → Server 认证 token → 校验 agent_id 与 host 绑定
  → 注册到 connection_registry（活跃连接表）
  → Agent 进入心跳循环（Heartbeat 每 N 秒）
```

## 2. 命令下发与执行（反向模式）

```text
控制台 → HTTP POST /api/v1/hosts/{id}/jobs
  → application.task_service 创建 Job（状态 = queued）
  → 通过 connection_registry 找到该 host 的活跃流
  → 流上推送 ExecRequest(command, timeout)
  → Agent 执行 → 流式回报 stdout/stderr 分块 + 退出码
  → Server 落库 Job 输出 → 前端轮询/订阅结果
```

## 3. 状态上报

```text
Agent 定时（或 Server 拉取触发）
  → 采集 CPU/内存/磁盘/进程/网络
  → 流上推送 MetricReport
  → Server 落库（时间序列）→ 前端看板
```

## 4. 文件传输

```text
控制台发起（上传/下载）
  → Server 创建 FileTransfer 任务
  → 流上下发指令（方向、路径、大小、分块策略）
  → 分块传输 + 进度回报 → 校验和 → 完成落库
```

## 5. 正向连接（同区域内网，Server 主动连 Agent）

```text
Agent 以 forward 模式监听 gRPC 端口
  → Server 按需（或调度器）作为 gRPC client 拨号 Agent
  → 建立双向流，后续信令与反向模式同构
  → 连接池/生命周期由 Server 管理
```

## 关键不变量

- 一条双向流是**单一事实来源**：Agent 在线 = 存在活跃流。
- 断线重连后重放 Register + 未完成的 Job 结果，不丢状态。
- 所有信令幂等：Job/FileTransfer 有稳定 ID，重复上报去重。
