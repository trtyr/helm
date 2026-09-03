# 未决问题

Role: open-questions
Status: active

仅列未解决问题。已解决/已决策的移入 `decisions/` 或标注去向。

1. **JWT 续期**。后端 JWT 24h 过期、无 refresh token 端点——过期即重新登录。
   需要续期机制吗（后端加 refresh 端点 / 前端临近过期提示重登）？→ 后端 backlog 候选。

2. **用户改密端点缺失**。users 表有 password_hash 但无 PUT /users/me 等端点；
   设置页暂无改密入口。→ 后端 backlog 候选。

3. **定时任务管理端点缺失**。tasks 无 DELETE / 暂停端点；主机任务页只能只读展示
   定时任务。→ 后端 backlog 候选（对齐后，前端补行内操作）。

4. **文件传输进度反馈**。upload/download 为同步 HTTP（Server 侧中转），
   前端无真实分块进度（规格已按「传输中… + 完成翻转」诚实处理）。
   若要真进度需后端分块协议或轮询 file_transfers 表。→ 后端 backlog 候选。

5. **WS 订阅粒度**。metrics/stream 为全局广播（前端按 host_id 过滤丢帧），
   大主机量下冗余流量；notifications/stream 单例无参数订阅 OK。
   → 后端 backlog 候选（按 host 订阅参数）。

6. **geistcn 与 React 19 兼容性**。官方组件库较新，脚手架期实测；降级路径
   见 [决策 005](decisions/005-component-strategy.md)（自建基础件 ≤15 个）。

7. **i18n**。当前产品语言中文（文案全部硬编码中文）；i18n 框架（react-intr 等）
   是否预留？倾向：不预留，等真实需求（YAGNI），抽取成本后置可控。

8. **仪表盘统计的告警计数口径**。F07「今日告警」需要按日过滤，但 GET /alerts
   无日期过滤参数（仅分页）——前端取第一页近似 or 后端加参数。倾向后端加
   `?since=` 参数。→ 后端 backlog 候选。
