# 002 — 设计语言：Vercel/Geist（暗色优先）

Date: 2026-09-03

## Context

用户明确：「Tactical RMM 太丑」，偏好 Vercel 审美——黑白灰、锐利、克制、留白充足。
Vercel 已开源 Geist 设计系统（官方文档 vercel.com/geist + npm `@vercel/geistcn` 组件库
与 `@vercel/geistcn-assets` 图标），tokens 有官方权威定义。

## Decision

- 采用 **Geist 设计语言**：10 阶灰阶语义系统、Geist Sans/Mono 字体、
  materials 圆角（6/8/12/16）与浮层阴影、官方 typography class 体系。
- **暗色为默认主题**，亮色切换；tokens 以 CSS 变量（`--ds-*`）承载，
  组件只消费变量。
- 全部具体值与布局规范见 [topics/design-language](../topics/design-language.md)
  （色板 hex、字号阶梯、8pt 间距、动效、全局框架）。

## Consequences

### 启用

- 视觉与交互有单一权威来源，杜绝随意取色/取距；
  与参考产品（Vercel dashboard）心智一致，「像 Vercel」即验收标准之一。

### 约束/代价

- Geist 是「工具审美」：强信息密度、弱装饰——营销风的渐变/大插画一律不用。

**相关：** [005 组件方案](005-component-strategy.md)、[design-language](../topics/design-language.md)
