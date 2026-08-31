# 通信协议

Role: topic-capsule
Status: planning
Read when: 需要了解 Server↔Agent 的协议底座与契约设计
Related: [decisions/001](../decisions/001-grpc-as-communication-base.md)

## One-Screen Summary

通信底座用 **gRPC（tonic）**。protobuf 契约是 Server 与 Agent 之间的单一事实来源，
定义在 `proto/agent/v1/`。双向流承载全部信令。

## 契约草案（`agent.proto`）

```proto
service AgentService {
  // 反向模式：Agent 主动连入，建立双向流
  rpc OpenChannel(stream AgentMessage) returns (stream ServerMessage);
}

message AgentMessage {
  oneof kind {
    Register        register        = 1;
    Heartbeat       heartbeat       = 2;
    MetricReport    metric_report   = 3;
    ExecResult      exec_result     = 4;  // 流式 stdout/stderr 分块
    FileChunk       file_chunk      = 5;
    FileStatus      file_status     = 6;
  }
}

message ServerMessage {
  oneof kind {
    RegisterAck     register_ack    = 1;
    ExecRequest     exec_request    = 2;
    FileRequest     file_request    = 3;
    MetricRequest   metric_request  = 4;  // 触发采集
  }
}
```

## Current Position

proto 未落地，上述为设计草案。`types.proto` 承载 Host/Task/Job 等共享类型。

## Active Constraints

- 只做向后兼容变更，破坏性变更走 `v2` 服务（`buf breaking` 门禁）。
- 所有消息带稳定 ID（job_id / transfer_id / request_id）以支持幂等重放。
- 信令语义化：Server 发「执行命令」语义，不关心平台实现差异。

## Open Risks Or Questions

- proto 物理位置（workspace 内 crate vs 独立 repo）→ open-questions#2
- 控制台走 REST 还是 gRPC-web → open-questions#1

## Details

- 契约演进门禁见 [baseline 门禁](../../../baseline/test-and-release-gates.md)
