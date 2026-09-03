# 006 — 图表：轻量 SVG 自绘（sparkline）+ uPlot（时序主图）

Date: 2026-09-03

## Context

图表需求两类：迷你 sparkline（仪表盘/指标次要项，无轴）与时序折线主图
（指标页 4 卡 + 阈值线 + hover 十字）。数据规模：60–288 点/卡，实时追加。
Vercel 风图表极度克制（细线、淡填充、无渐变）。

## Decision

- **sparkline：纯 SVG 自绘**（polyline + 可选面积），≤60 行组件，零依赖。
- **时序主图：uPlot**（~48KB，为时序而生：百万点性能、十字光标、缩放内置），
  包一层 React hook（`useUplot(series, opts)`）+ Geist 主题预设
  （线色/轴字/网格色全部走 `--ds-*`）。
- 不用 ECharts/Recharts：前者体积与默认风格与 Vercel 审美冲突大（定制成本高），
  后者性能与十字光标定制弱于 uPlot。
- 环形图（仪表盘在线率）：SVG 自绘（stroke-dasharray），不进图表库。

## Consequences

### 启用

- 视觉完全受控（Geist tokens 直出）；实时追加（setData 局部更新）流畅。
- 依赖面小：uPlot 无 React 绑定耦合，包一层即可。

### 约束/代价

- uPlot API 命令式——hook 封装需一次性好；hover tooltip 需自绘浮层（值 + 时间）。

**相关：** [002 设计语言](002-design-language-vercel-geist.md)、[metrics 路由](../routes/metrics.md)
