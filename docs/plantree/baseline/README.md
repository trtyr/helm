# Baseline — 项目级上下文

Role: entrypoint
Status: active

Helm 运维平台的项目级上下文。plan root 通过链接引用这里，不复制全局内容。

当前项目**已实现落地**（Server 控制端 + Agent 被控端 + 共享 proto 均可运行）。
baseline 记录的是**已实现的事实**；与实现不符的「目标设计」措辞已同步修正。

## 文件地图

| 文件 | 内容 |
|------|------|
| [module-map.md](module-map.md) | 模块地图：Server 分层、Agent、共享 proto |
| [runtime-flows.md](runtime-flows.md) | 运行时流程：连接、命令、状态、文件、正/反向 |
| [storage-and-state.md](storage-and-state.md) | 存储选型、实体模型、状态机、迁移机制 |
| [test-and-release-gates.md](test-and-release-gates.md) | 测试分层与发布门禁 |
| [risk-hotspots.md](risk-hotspots.md) | 风险热点与缓解方向 |
| [capability-matrix.md](capability-matrix.md) | C2 能力现状矩阵（已实现/缺失对照） |
