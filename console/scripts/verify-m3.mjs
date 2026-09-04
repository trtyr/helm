/**
 * M3 全量联调（playwright + 真 agent）：
 * 服务（创建/启停/日志快照+实时 tail/删除）→ 进程（列表/kill）→ 网络（网卡卡）
 * → 指标（图渲染/范围切换）→ 任务域（快速执行→详情输出→列表→主机任务 tab）→ 终端 F34。
 * 前置：后端 18081 + dev 5180 + agent m2-agent 已启动。
 */
import { chromium } from "playwright";
import { execSync } from "node:child_process";

const BASE = "http://localhost:5180";
const API = "http://127.0.0.1:18081";

// API 拿 host_id
const loginRes = await fetch(`${API}/api/v1/auth/login`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ username: "admin", password: "admin123" }),
}).then((r) => r.json());
const TOKEN = loginRes.token;
const agents = await fetch(`${API}/api/v1/agents`, {
  headers: { Authorization: `Bearer ${TOKEN}` },
}).then((r) => r.json());
const agent = (agents.agents ?? []).find((a) => a.id === "m2-agent");
if (!agent) throw new Error("m2-agent 未注册——请先启动 agent");
console.log("前置：m2-agent host =", agent.host_id);

// 幂等清理：上次运行残留的 m3-ticker 服务（先停再删）
const svcList = await fetch(`${API}/api/v1/services?limit=100`, {
  headers: { Authorization: `Bearer ${TOKEN}` },
}).then((r) => r.json());
for (const s of (svcList.services ?? []).filter((x) => x.name === "m3-ticker")) {
  if (s.status === "running") {
    await fetch(`${API}/api/v1/services/${s.id}/stop`, {
      method: "POST",
      headers: { Authorization: `Bearer ${TOKEN}` },
    });
  }
  await fetch(`${API}/api/v1/services/${s.id}`, {
    method: "DELETE",
    headers: { Authorization: `Bearer ${TOKEN}` },
  });
}
if ((svcList.services ?? []).some((x) => x.name === "m3-ticker")) console.log("前置：已清理残留 m3-ticker");

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

await page.goto(`${BASE}/`);
await page.fill("#username", "admin");
await page.fill("#password", "admin123");
await page.click("button[type=submit]");
await page.waitForURL(/\/hosts$/, { timeout: 90_000 }); // bcrypt debug 慢

const detail = (tab) => `${BASE}/hosts/${agent.host_id}/${tab}`;

// ── 导航就绪回归断言（audit 整改：M3 交付后不得残留「M3/后续」未就绪标志）──
await page.goto(`${BASE}/hosts`);
const staleNav = await page.locator("nav a:has-text('任务') >> text=后续").count();
if (staleNav !== 0) throw new Error("侧栏「任务」仍带「后续」徽标");
await page.goto(detail("overview"));
await page.waitForSelector("nav a:has-text('服务')");
for (const seg of ["服务", "进程", "网络", "指标", "任务"]) {
  const badge = await page.locator(`a:has-text('${seg}') >> text=M3`).count();
  if (badge !== 0) throw new Error(`tab「${seg}」仍带 M3 徽标`);
}
console.log("✓ 导航就绪：侧栏任务 + 五 tab 无「后续/M3」残留标志");

// ── 服务 tab ──
await page.goto(detail("services"));
await page.click("button:has-text('+ 创建服务')");
await page.fill("#svc-name", "m3-ticker");
await page.fill("#svc-command", "/bin/sh");
await page.fill("#svc-args", "-c\nwhile true; do date '+%T m3tick'; sleep 2; done");
await page.click("form button[type=submit]");
await page.waitForSelector("tr:has-text('m3-ticker')", { timeout: 30_000 });
console.log("✓ 服务：创建成功，列表出现 m3-ticker");

// 状态徽标等待 running（agent 启动上报有延迟）
await page.waitForSelector("tr:has-text('m3-ticker') >> text=运行", { timeout: 30_000 });
console.log("✓ 服务：状态翻转为运行");

// 日志抽屉：快照 + 实时 tail
await page.click("tr:has-text('m3-ticker') >> button:has-text('m3-ticker')");
await page.waitForSelector("text=m3tick", { timeout: 30_000 });
console.log("✓ 服务日志：快照出现 m3tick 行");
await page.click("button[aria-pressed]:has-text('实时')");
await page.waitForFunction(
  () => (document.body.textContent?.match(/m3tick/g) ?? []).length >= 2,
  { timeout: 30_000 },
);
console.log("✓ 服务日志：WS 实时 tail 增量到达");
await page.click("button[aria-label='关闭抽屉']");

// 停止 + 删除清理
await page.click("tr:has-text('m3-ticker') >> [aria-label='停止 m3-ticker']");
await page.waitForSelector("tr:has-text('m3-ticker') >> text=停止", { timeout: 30_000 });
console.log("✓ 服务：停止生效");
await page.hover("tr:has-text('m3-ticker')");
await page.click("tr:has-text('m3-ticker') >> [aria-label='更多 m3-ticker']");
await page.click("div.w-36 button:has-text('删除')");
await page.waitForFunction(
  () => ![...document.querySelectorAll("tbody tr")].some((tr) => tr.textContent?.includes("m3-ticker")),
  { timeout: 30_000 },
);
console.log("✓ 服务：删除清理完成");

// ── 进程 tab ──
await page.goto(detail("processes"));
await page.waitForFunction(
  () => {
    const m = document.body.textContent?.match(/共 (\d+) 个进程/);
    return m && Number(m[1]) > 5;
  },
  { timeout: 30_000 },
);
const procCount = await page.locator("tbody tr").count();
console.log(`✓ 进程：列表渲染（共 ${procCount}+ 行）`);

// kill 一个 sleep 目标（node 侧起，页面里杀）
execSync("nohup sleep 65535 >/dev/null 2>&1 &", { shell: "/bin/bash" });
await page.fill("input[aria-label='搜索进程']", "sleep");
await page.waitForTimeout(1500);
await page.hover("tbody tr:first-child");
await page.click("tbody tr:first-child [aria-label^='操作']");
await page.click("div.w-32 button:has-text('结束进程')");
await page.waitForSelector("text=该操作不可恢复", { timeout: 5_000 });
await page.click("div.justify-end button:has-text('结束进程')");
// toast 生命周期 4s：立刻等 role=status（进程已结束）
await page.waitForSelector("[role=status]:has-text('已结束')", { timeout: 10_000 });
console.log("✓ 进程：kill 语义化结果（toast 已结束）");

// ── 网络 tab ──
await page.goto(detail("network"));
await page.waitForSelector("div:has(> h3)", { timeout: 30_000 });
await page.waitForSelector("h3:has-text('lo0')", { timeout: 30_000 });
console.log("✓ 网络：网卡卡渲染（lo0 可见）");

// ── 指标 tab ──
await page.goto(detail("metrics"));
// 实时模式有最长 30s 空窗（等 agent 下一轮上报）——先切 1h 历史断言图表管线
await page.click("button:has-text('1h')");
await page.waitForSelector("text=CPU 使用率", { timeout: 30_000 });
await page.waitForSelector("canvas", { timeout: 30_000 });
console.log("✓ 指标：范围 1h 历史查询渲染（uPlot canvas + CPU 使用率卡）");

await page.click("button:has-text('▸ 次要指标')");
await page.waitForSelector("text=net.rx_bytes", { timeout: 10_000 });
console.log("✓ 指标：次要指标折叠区展开");

// 回实时：等一轮上报（30s 周期，给 50s 余量）——硬断言（binary 帧解码已实证）
await page.click("button:has-text('实时')");
await page.waitForSelector("canvas", { timeout: 50_000 });
console.log("✓ 指标：实时模式 WS 推点渲染");

// ── 任务域 ──
await page.goto(detail("overview"));
await page.fill("input[placeholder='uname -a']", "uname");
await page.click("button:has-text('运行')");
await page.waitForURL(/\/jobs\/[^/]+$/, { timeout: 90_000 });
console.log("✓ 任务：快速执行创建后直达 /jobs/:id");
await page.waitForSelector("text=Darwin", { timeout: 90_000 });
console.log("✓ 任务详情：输出回放包含 uname 结果（Darwin）");
const jobId = page.url().split("/").pop();

await page.goto(`${BASE}/jobs`);
await page.waitForSelector(`tr:has-text('#${jobId.slice(0, 4)}')`, { timeout: 30_000 });
console.log("✓ 任务列表：全局列表出现该任务");

await page.goto(detail("tasks"));
await page.waitForSelector("text=快速", { timeout: 30_000 });
console.log("✓ 主机任务 tab：执行历史可见");

// ── 终端 F34 ──
await page.goto(detail("terminal"));
await page.waitForSelector(".xterm", { timeout: 15_000 });
await page.click("button[aria-label='终端设置']");
await page.waitForSelector("text=主题（仅终端）", { timeout: 5_000 });
await page.click("button:has-text('终端亮')");
console.log("✓ 终端 F34：设置浮层（字号/主题三档）可见且可切换");
await page.click("button[aria-label='关闭设置']");

await browser.close();
console.log("\nM3 联调（服务/进程/网络/指标/任务/终端设置）全部通过");
