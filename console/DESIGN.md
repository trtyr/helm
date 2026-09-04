# DESIGN.md — Geist / Vercel 视觉世界

## World

Vercel **Geist** 设计语言（官方开源系统，vercel.com/geist）。黑白灰即品牌；
色彩只做语义；1px 锐利边框 + 小圆角；克制的 150–200ms 微动效；暗色为第一主题。

## Authority

- Token 权威：`src/styles/tokens.css`（`--ds-*` CSS 变量，暗/亮两组），
  数值来源 helm 仓 `docs/plantree/plans/frontend/topics/design-language.md`。
- 字体：Geist Sans（UI）/ Geist Mono（机器数据：ID/IP/数值/时间/终端）。
- 布局骨架：Topbar 56px + Sidebar 220px（可折叠 64px）+ StatusBar 28px。
- 组件：Tailwind v4 utilities + tokens 变量；基础件用 `@vercel/geistcn`（兼容性
  不佳则按同 tokens 自建）。

## Hard bans

- 大圆角（>16px）、大阴影、渐变装饰、彩色插画、页面转场动画。
- 写死 hex（必须走 `--ds-*` 变量）；正文中滥用 Bold（层级靠字号/灰阶）。
- 表格数字不用 tabular-nums。

## Modes

全部 surface 为 **Operate**（工具界面）：扫描性、一致性、真实使用场景优先；
brand 住在细节里（间距、对齐、mono 的使用纪律）。
