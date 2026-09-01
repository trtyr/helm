# Plan: Server（后端控制端）

Role: entrypoint（plan root）
Status: active

集中式运维平台的**后端**计划。以运维平台能力为主线（参考 C2 控制面，不做隐蔽性设计），覆盖 Server 控制端 + Agent 被控端 + 共享 proto。前端控制台独立工程，不在本树。

## 核心原则（用户明确要求）

> 不要为了出 MVP 而放弃架构。MVP 是功能上的 MVP，但**底座一开始就建好**。

具体含义：

- 通信底座直接上 **gRPC**（强类型契约、双向流、自带认证/超时/重试），不上 WebSocket+JSON。
- 分层架构（domain/application/适配器）从一开始就立起来，不为省事把逻辑堆进 handler。
- 存储 schema、迁移机制、认证、可观测性都按生产标准，功能可以后补，底座不返工。

## 文件地图

| 文件 | 角色 |
|------|------|
| [roadmap.md](roadmap.md) | 当前状态与阶段 |
| [open-questions.md](open-questions.md) | 未决问题 |
| [topics/](topics/README.md) | 主题胶囊（通信、连接、模块、数据、安全） |
| [decisions/](decisions/README.md) | 稳定决策链 |

## 阅读路径

1. 先读 [roadmap.md](roadmap.md) 了解阶段划分（Phase 0/1 是底座）。
2. 读 [decisions/](decisions/README.md) 了解已钉死的技术选型与理由。
3. 按需钻入 [topics/](topics/README.md) 看具体设计。
4. 未决项见 [open-questions.md](open-questions.md)。
