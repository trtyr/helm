# Frontend Plan — helm 控制台（Vite + React，Geist 设计语言）

Role: plan-root
Status: active（设计规格已完成；实现 M1/M2 已落地，按里程碑推进）

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
| **M1 骨架**（tokens/布局/登录/主机列表） | ✅ 已落地 | github.com/trtyr/helm-console `762800f`：playwright 登录流（含已登录重定向）+ 主机 CRUD 全流程联调通过；TS strict 落配置层；vitest 8 测试；build/lint 零错误 |
| **M2 详情**（详情框架/概览/终端/文件） | ✅ 已落地 | helm-console `1860274`：verify-m2.mjs 14 项联调全过（终端回显 + resize 双向同步实证 tput cols、文件上传/下载 checksum 逐字节往返、离线守卫 GUARDED 区分）；后端配套 `a4b3848`（terminal WS resize 透传 + fs.rs is_dir follow symlink 修复）；vitest 14 测试 |
| **M3 监控与任务**（服务/进程/网络/指标/任务 + uPlot） | ✅ 已落地 | helm-console `68083c1`（初 6f22974 经 audit 整改：导航就绪翻转 + JobDetail 重连降级/运行计时补全）：verify-m3.mjs 18 项真 agent 联调全过（服务启停 + 日志快照/WS tail、进程 kill toast、网卡卡、指标历史 1h + 实时 WS 推点、任务创建直达详情 + 输出回放、终端 F34 主题/全屏）；uPlot 按决策 006 集成；全局 toast + useWsStream/useBinaryStream（修 StreamRegistry binary 帧解码）；vitest 35 测试 |
| M4+（通知/告警/审计/监听器/设置/仪表盘…） | 未开工 | 按 routes/ 规格逐里程碑推进 |

## 边界

- 本 plan 覆盖**设计规格**（本仓库）；前端代码在新仓 helm-console。
- 后端因本规划发现的 API 缺口（open-questions 1–5）回投后端 backlog 评估，
  不阻塞前端按现契约开发（各规格已注明降级处理）。
