# 路由地图 — 全部页面规格索引

Role: index
Status: active
Read when: 开发任何页面前读对应规格；核对功能↔路由覆盖时读本表
Related: [功能清单](../topics/feature-inventory.md)、[设计语言](../topics/design-language.md)

## 路由总表

20 个路由（登录 1 + 应用内 19；主机详情为 1 个布局框架 + 8 个 tab 路由）。
除 `/login` 外全部受保护（JWT）；未认证访问重定向 `/login?redirect=`。

| 路由 | 名称 | 规格文档 | 承载功能 |
|------|------|---------|---------|
| `/login` | 登录 | [login.md](login.md) | F01 |
| `/dashboard` | 仪表盘（默认首页 `/` 重定向至此） | [dashboard.md](dashboard.md) | F06, F07–F11 |
| `/hosts` | 主机列表（tabs：主机 / Agents） | [hosts.md](hosts.md) | F12–F14, F20–F24 |
| `/hosts/:id` | 主机详情布局框架（tabs 容器） | [host-detail.md](host-detail.md) | 框架 |
| `/hosts/:id/overview` | 主机概览（默认 tab） | [host-overview.md](host-overview.md) | F14, F18, F21, F25, F49* |
| `/hosts/:id/terminal` | 交互终端 | [terminal.md](terminal.md) | F32–F34 |
| `/hosts/:id/files` | 文件管理 | [files.md](files.md) | F35–F39 |
| `/hosts/:id/services` | 服务管理 | [services.md](services.md) | F40–F45 |
| `/hosts/:id/processes` | 进程管理 | [processes.md](processes.md) | F46–F48 |
| `/hosts/:id/network` | 网络信息 | [network.md](network.md) | F49 |
| `/hosts/:id/metrics` | 指标监控 | [metrics.md](metrics.md) | F50–F53 |
| `/hosts/:id/tasks` | 任务历史 | [host-tasks.md](host-tasks.md) | F25, F29–F31 |
| `/jobs` | 全局任务列表 | [jobs.md](jobs.md) | F26 |
| `/jobs/:id` | 任务详情 | [job-detail.md](job-detail.md) | F27, F28 |
| `/notifications` | 通知中心 | [notifications.md](notifications.md) | F59, F60, F61* |
| `/alerts` | 告警 | [alerts.md](alerts.md) | F54–F56 |
| `/audit` | 审计 | [audit.md](audit.md) | F62, F63 |
| `/listeners` | 监听器 | [listeners.md](listeners.md) | F64–F68 |
| `/settings` | 设置 | [settings.md](settings.md) | F04, F69 |
| `/forward` | 正向快捷执行 | [forward.md](forward.md) | F70 |

> 带 `*` 的功能主承载在别处，该路由是次要消费点。
> 全局元素（Topbar 铃铛/搜索/主题、StatusBar）见 [设计语言·全局布局](../topics/design-language.md)，
> 归属功能 F02, F04, F05, F06, F57, F58, F61 不重复计入路由。

## 路由层级与守卫

```text
/login                     公开
/ (受保护布局：Topbar + Sidebar + StatusBar)
├── /dashboard
├── /hosts                 （tabs: hosts | agents 两个查询参数视图）
├── /hosts/:id/*           （详情布局：主机头 + 8 tabs）
├── /jobs, /jobs/:id
├── /notifications
├── /alerts
├── /audit
├── /listeners
├── /settings
└── /forward
```

## 通用状态规范（所有路由继承）

每个路由规格的状态矩阵遵循统一基线，个别路由只写差异：

| 状态 | 规范 |
|------|------|
| Loading | 骨架屏（骨架色 gray-100/200 交替 pulse）；列表 = 5 行骨架，图表 = 灰块 |
| Empty | 居中插图位（图标 20px gray-900）+ copy-13 说明 + 主操作按钮（若有） |
| Error | 页内错误卡（非模态）：red-1000 标题 + 错误码 mono + 「重试」按钮；不整页白屏 |
| 404 | 路由级：Vercel 风「404」heading-64 + 返回首页链接 |
| 401 | 全局拦截（F03），路由不自管 |
