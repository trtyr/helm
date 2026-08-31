# Baseline — 项目级上下文

Role: entrypoint
Status: active

Helm 运维平台的项目级上下文。plan root 通过链接引用这里，不复制全局内容。

当前项目为**空壳**（仅 `src/main.rs` 的 hello world），尚无代码落地。
因此 baseline 记录的是**目标架构设计**与当前现状，而非已有实现的事实。

## 文件地图

| 文件 | 内容 |
|------|------|
| [module-map.md](module-map.md) | 目标模块地图：Server 分层、Agent、共享 proto |
| [runtime-flows.md](runtime-flows.md) | 运行时流程：连接、命令、状态、文件、正/反向 |
| [storage-and-state.md](storage-and-state.md) | 存储选型、实体模型、状态机、迁移机制 |
| [test-and-release-gates.md](test-and-release-gates.md) | 测试分层与发布门禁 |
| [risk-hotspots.md](risk-hotspots.md) | 风险热点与缓解方向 |
