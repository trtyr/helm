# 路由：/dashboard — 仪表盘

Role: route-spec
Status: active
Implemented: M4 落地（helm-console 3d1d70a，验收见对应里程碑 verify-m*.mjs）
Related: [路由地图](README.md)、[功能 F06–F11](../topics/feature-inventory.md)

## 功能清单

- F07 概览统计卡（4 联）
- F08 在线率环形图
- F09 最近通知（8 条）
- F10 最近告警（5 条）
- F11 全局实时 CPU sparkline
- F06（StatusBar WS 状态在全局层，本页消费其数据可用性）

## 布局与留白

```text
┌ Content（max-w 1440 居中，左右 32px）───────────────────────────────┐
│ 概览                                          heading-32          │
│ 主机状态与活动总览                  copy-13 gray-900    32px↓     │
│ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐   gap 24px            │
│ │ 主机    │ │ 在线    │ │ 未读通知│ │ 告警*  │   卡片 h-96           │
│ │   12   │ │  10    │ │    3   │ │   5   │   数字 heading-32     │
│ │ 台主机  │ │ 台在线  │ │ 条未读 │ │ 今日  │   mono tabular        │
│ └────────┘ └────────┘ └────────┘ └────────┘   圆角 8 边框 1px     │
│                                          32px↓                    │
│ ┌──────────────────────┐  ┌──────────────────────────────────┐   │
│ │ 在线率      24px↓     │  │ 全局 CPU（实时）                  │   │
│ │    ╭───╮             │  │  ▁▂▂▃▅▇▆▅▃▂▂▃▅▇  sparkline       │   │
│ │    │ 83% │  环形图    │  │  12 台均值 · 60s 窗口  label-12   │   │
│ │    ╰───╯ 10/12       │  │                                  │   │
│ │ 离线 2 · stale 0      │  └──────────────────────────────────┘   │
│ └──────────────────────┘                                            │
│                                          32px↓                     │
│ ┌──────────────────────┐  ┌──────────────────────────────────┐   │
│ │ 最近通知        全部→ │  │ 最近告警                    全部→ │   │
│ │ ● 主机 web-1 已上线   │  │ web-2  cpu.usage  92.1 → 90     │   │
│ │ ● 主机 db-1 已下线    │  │ web-1  disk.usage  96.4 → 90     │   │
│ │ ● 预警：cpu…（2m）    │  │ …                                │   │
│ └──────────────────────┘  └──────────────────────────────────┘   │
└────────────────────────────────────────────────────────────────────┘
```

栅格：12 划分——统计卡 4×3 列；第二行 环形图 5 列 + sparkline 7 列；第三行 通知 5 列 + 告警 7 列。卡片间距一律 24px（space-6）。

## 组件与细节

- **统计卡**：label-13 标题（gray-900）+ heading-32 数字（mono，tabular-nums）+
  label-12 单位；可点击（主机→/hosts、未读→/notifications、告警→/alerts），
  hover 边框 gray-500；未读通知卡带 blue-1000 圆点角标。
- **环形图**：纯 SVG（80px），底环 gray-700、进度环 green-1000；中心 heading-24 百分比。
- **sparkline**：WS metrics/stream 聚合（在线主机 cpu.usage 均值），60 点环形缓冲；
  polyline 1.5px blue-1000 + 8% 面积填充；左上实时值 label-13-mono。
- **迷你通知**：行 = kind 圆点（8px，色按 kind：online green/offline gray/alert amber）+
  message 截断 + 相对时间 label-12 gray-900；行高 32px hover gray-200。
- **迷你告警**：hostname + metric_name（mono）+ value（mono red-1000）+ 阈值箭头。
- 数据加载：并行 3 请求（hosts / notifications / alerts）；WS 仅 metrics。

## 状态矩阵

| 状态 | 表现 |
|------|------|
| Loading | 4 统计卡骨架（数字位灰条）+ 图表区灰块 + 列表 5 行骨架 |
| Empty（0 主机） | 全页空态：主机图标 + 「还没有主机」+ 「创建主机」按钮（跳 /hosts） |
| Empty（有主机 0 通知/告警） | 对应卡片内空态「暂无通知」/「暂无告警」label-13 gray-900 |
| Error（API 失败） | 对应卡片变错误卡（标题 + 重试）；互不拖垮——通知挂了告警照常 |
| WS 断线 | sparkline 卡右上红点 + 「实时已断开」label-12；数据冻结不闪烁 |

## 交互细节

- 页面不自动轮询 HTTP；WS 连接恢复时 sparkline 清空重算（避免断点折线）。
- 相对时间（2m / 1h / 3d）每 30s 前端重渲染一次。
- 「全部 →」链接：label-13 blue-1000，hover 下划线。
