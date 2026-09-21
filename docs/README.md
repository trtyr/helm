# docs/ — helm 文档指针索引

> **2026-09-15 起，helm 的全部书面记录归档于 engram（projects 域，project `helm`）。**
> 本地 `docs/` 只保留本索引与机器契约资产。正文不再放本地——查文档一律走 engram。
>
> **2026-09-21 刷新**：分类栏补齐「决策」「历史」；各栏补全 2026-09-16/17 与 09-20
> 两轮 arc42 对齐新增的篇目；计数按 HEAD `5fcf659` 实测更正（原「17 表 / 16 迁移」
> 已过期，即工单 EN-75，本次关闭）。

## 怎么查

- engram MCP：`projects` 工具，`project_name = "helm"`
  - `doc_search`：跨文档按行检索（grep 式）
  - `doc_get`：按 doc_id 精读（映射表里有每个文档的 doc_id）
- 分类结构：总览 / 架构与设计 / 接口契约 / 数据模型 / 运行与部署 / 约定 /
  现状与门禁 / 应急响应 / 规划 / 决策 / 历史

## 分类 → 内容对照

| engram 分类 | 内容（原本地文件） |
|---|---|
| 总览 | 项目概览、overview、README（原归档索引）、术语表、helm 全景与演进判断（2026-09-20）、本地归档 → engram 映射表 |
| 架构与设计 | 模块地图（arc42 §5）、architecture（目录树/模块边界/运行时流程）、运行时视图、系统上下文、tech-stack（依赖版本）、实证地图（2026-09-20 实测） |
| 接口契约 | API 与接口、api（gRPC 契约 + HTTP API + 环境变量）、mcp（MCP 接入：`POST /mcp`，41 op） |
| 数据模型 | 数据模型、data-model（**18 表 / 21 迁移** / 状态机） |
| 运行与部署 | 部署与运维、run-and-deploy（运行/开发命令/e2e/部署模板）、real-machine-test-report（真机 15/15） |
| 约定 | conventions（分层/错误处理/命名/测试/契约演进/git 约定） |
| 现状与门禁 | 测试与门禁、current-state（实测门禁基线）、文档 vs 代码差异清单（2026-09-15）、代码缺口清单（2026-09-15） |
| 应急响应 | ir-capabilities（IR 全能力：自启动/内存扫描/时间线/证据包） |
| 决策 | 技术决策记录（S/F/C 三组决策账） |
| 历史 | 风险与技术债、开工记录（2026-09-16 / 2026-09-18 风险债大扫除）、更新记录（2026-09-20 / 2026-09-20 代码健康大修 / 2026-09-21 文档对齐） |
| 规划 | 原 plantree 全树 75 篇（README / baseline 6 / roadmap / decisions 001–011 / topics / frontend 全套），folder 保留原目录结构 |

## 本地保留资产

- `docs/openapi.yaml` — HTTP API 契约（OpenAPI 3.0.3，72 端点）；
  `scripts/check_openapi.py` 消费，勿迁。
  2026-09-21 补齐三处与实现不一致的表述：登录 429 退避响应；
  `change-password` / `change-username` 成功后旧 JWT 立即失效（`users.token_version`）；
  `/api/v1/agents/cert` 的 403（CSR 主体 CN 必须等于 `agent_id`）。
  前端类型由 `pnpm --dir console run api:gen` 从本文件生成
  （`console/src/api/schema.d.ts`，勿手改）。
- 本文件 — 指针索引。

## 历史备注

- 原 docs 归档 12 篇 + plantree 75 篇于 2026-09-15 逐字迁入 engram
  （全量 doc_get 与原文 diff 一致后删除本地）。
- **EN-75 关闭（2026-09-21）**：旧索引中「data-model（17 表 / 16 迁移）」
  「缺决策/历史两栏」「mcp.md 未入册」等过期项已按 HEAD `5fcf659` 实测更正；
  明细见 engram「历史 / 风险与技术债」与「历史 / 更新记录 2026-09-21 文档对齐」。
