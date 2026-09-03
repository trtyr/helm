# Frontend Plan — helm 控制台（Vite + React，Geist 设计语言）

Role: plan-root
Status: active（设计规格已完成，实现未开工）

helm 前端控制台的完整设计规格。后端接口契约见
[docs/openapi.yaml](../../../openapi.yaml)（44 HTTP + 5 WS，Phase 9 后）；
后端规划见 [../server](../server/README.md)。前端实现仓库（helm-console）建立后，
本 plan root 迁移或双链。

## 文件地图与阅读路径

1. **先读**：[topics/design-language.md](topics/design-language.md) — Geist tokens、
   色板、字号、间距、全局布局框架（一切页面规格的引用基座）。
2. **再读**：[topics/feature-inventory.md](topics/feature-inventory.md) —
   70 个功能点（14 域）+ 后端 API 全映射核对表。
3. **按页读**：[routes/README.md](routes/README.md) — 20 路由总表 →
   每路由一份规格（功能 / ASCII 布局线框 / 留白 tokens / 状态矩阵 / 交互细节）。
4. **实现前读**：[decisions/README.md](decisions/README.md) — 技术栈与架构决策 001–007。
5. **未决项**：[open-questions.md](open-questions.md) — 含后端 API 缺口的
   backlog 候选（JWT 续期 / 改密 / 定时任务管理 / 传输进度 / 告警日期过滤）。

## 状态

| 产出 | 状态 | 证据 |
|------|------|------|
| 设计语言 tokens | ✅ | design-language.md（官方 Geist 规范对齐） |
| 功能清单（70）+ API 映射 | ✅ | feature-inventory.md（44 HTTP + 5 WS 无孤儿） |
| 路由规格（20） | ✅ | routes/（每路由四要素齐备） |
| 技术决策（7）+ 开放问题（8） | ✅ | decisions/ + open-questions.md |
| 前端实现 | 未开工 | 仓库 helm-console 尚未创建 |

## 边界

- 本 plan 覆盖**设计规格**（本仓库）；前端代码在新仓 helm-console。
- 后端因本规划发现的 API 缺口（open-questions 1–5）回投后端 backlog 评估，
  不阻塞前端按现契约开发（各规格已注明降级处理）。
