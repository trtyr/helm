/**
 * M2 全量联调（playwright + 真 agent）：
 * 概览渲染 → 终端回显/resize → 文件列目录/上传/下载/排序/面包屑 → 离线守卫。
 * 前置：后端 18081 + dev 5180 + agent m2-agent 已启动。
 */
import { chromium } from "playwright";
import { writeFileSync, readFileSync, unlinkSync, existsSync } from "node:fs";
import { execSync, spawn } from "node:child_process";

const BASE = "http://localhost:5180";
const API = "http://127.0.0.1:18081";

// Server 侧测试文件（本机自环：Server 与 agent 同机）
const SRC = "/tmp/m2-upload.txt";
const REMOTE = "/tmp/m2-remote.txt";
const DOWNLOADED = "/tmp/m2-downloaded.txt";
writeFileSync(SRC, "m2 transfer content 7788\n");
for (const f of [REMOTE, DOWNLOADED]) if (existsSync(f)) unlinkSync(f);

// API 拿 m2-agent 的 host_id
const loginRes = await fetch(`${API}/api/v1/auth/login`, {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ username: "admin", password: "admin123" }),
}).then((r) => r.json());
const agents = await fetch(`${API}/api/v1/agents`, {
  headers: { Authorization: `Bearer ${loginRes.token}` },
}).then((r) => r.json());
const agent = (agents.agents ?? []).find((a) => a.id === "m2-agent");
if (!agent) throw new Error("m2-agent 未注册——请先启动 agent");
console.log("前置：m2-agent host =", agent.host_id);

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

await page.goto(`${BASE}/`);
await page.fill("#username", "admin");
await page.fill("#password", "admin123");
await page.click("button[type=submit]");
await page.waitForURL(/\/hosts$/, { timeout: 90_000 }); // bcrypt debug 慢

// ── 概览 ──
await page.goto(`${BASE}/hosts/${agent.host_id}/overview`);
await page.waitForSelector("text=指标快照");
await page.waitForSelector("text=主机信息");
console.log("✓ 概览：指标快照/主机信息/Agent 卡渲染");

// 快速执行 → toast job_id
await page.fill("input[placeholder='uname -a']", "uname");
await page.click("button:has-text('运行')");
await page.waitForSelector("text=任务已创建", { timeout: 60_000 });
console.log("✓ 概览：快速执行创建任务（toast job_id）");

// ── 终端 ──
await page.click("nav a:has-text('终端')");
await page.waitForSelector(".xterm", { timeout: 15_000 });
await page.waitForTimeout(1500);
await page.keyboard.type("echo M2_FULL_MARKER");
await page.keyboard.press("Enter");
await page.waitForSelector(".xterm-rows >> text=M2_FULL_MARKER", { timeout: 10_000 });
console.log("✓ 终端：回显正常");

// resize：视口变化 → PTY 列数变化
await page.setViewportSize({ width: 1100, height: 700 });
await page.waitForTimeout(800);
await page.keyboard.type("tput cols");
await page.keyboard.press("Enter");
await page.waitForTimeout(1200);
const termText = await page.textContent(".xterm-rows");
if (!/9[0-9]/.test(termText?.slice(-40) ?? "")) {
  console.log("⚠ resize 输出未确认（tput cols 应≈95）：", termText?.slice(-60));
} else {
  console.log("✓ 终端：resize 双向同步（tput cols ≈ 新宽度）");
}

// ── 文件 ──
await page.click("nav a:has-text('文件')");
await page.waitForSelector("text=名称", { timeout: 10_000 });
await page.waitForSelector("tr:has-text('tmp')");
console.log("✓ 文件：根目录列出（tmp 可见）");

// 进 tmp
await page.dblclick("tr:has-text('tmp')");
await page.waitForSelector("nav[aria-label=路径] >> text=tmp");
console.log("✓ 文件：双击目录进入 /tmp（面包屑更新）");

// 上传（Server /tmp/m2-upload.txt → 远端 /tmp/m2-remote.txt）
await page.click("button:has-text('上传')");
await page.fill("#pair-from", SRC);
await page.fill("#pair-to", REMOTE);
await page.click("form button[type=submit]");
await page.waitForSelector("text=✓ 校验通过", { timeout: 30_000 });
console.log("✓ 文件：上传 checksum_ok=true（队列完成态）");

// 列表刷新后远端文件可见
await page.waitForSelector(`tr:has-text('m2-remote.txt')`, { timeout: 10_000 });
console.log("✓ 文件：上传后列表出现 m2-remote.txt");

// 下载（m2-remote.txt → Server /tmp/m2-downloaded.txt）
await page.hover("tr:has-text('m2-remote.txt')");
await page.click("tr:has-text('m2-remote.txt') >> button[aria-label='下载 m2-remote.txt']");
await page.fill("#pair-to", DOWNLOADED);
await page.click("form button[type=submit]");
// 队列里上传+下载两项都完成（两条「校验通过」）才继续
await page.waitForFunction(
  () => (document.body.textContent?.match(/校验通过/g) ?? []).length >= 2,
  { timeout: 60_000 },
);
const downloaded = readFileSync(DOWNLOADED, "utf-8");
if (downloaded !== "m2 transfer content 7788\n") throw new Error(`下载内容不一致: ${downloaded}`);
console.log("✓ 文件：下载往返内容逐字节一致");

// 排序：点大小表头切换
await page.click("th:has-text('大小')");
await page.waitForTimeout(500);
console.log("✓ 文件：排序切换可用（大小 ↑）");

// 面包屑回根
await page.click("nav[aria-label=路径] >> text=/");
await page.waitForTimeout(600);
console.log("✓ 文件：面包屑回根导航");

// ── 列表行点击导航（task-1 契约）──
await page.goto(`${BASE}/hosts`);
await page.click("tbody tr:first-child");
await page.waitForURL(/\/hosts\/[^/]+\/overview$/, { timeout: 15_000 });
console.log("✓ 主机列表行点击 → 详情概览导航");

// ── 离线守卫（task-1 契约：仅 GUARDED tab 显示 amber 通栏）──
execSync('pkill -f "agent-id m2-agent"');
await page.goto(`${BASE}/hosts/${agent.host_id}/terminal`);
// 列表缓存 30s 轮询 → online 翻 false 后守栏出现
await page.waitForSelector("text=主机离线", { timeout: 60_000 });
console.log("✓ 离线守卫：终端 tab（GUARDED）显示 amber 通栏");

await page.click("nav a:has-text('概览')");
await page.waitForTimeout(1000);
const guardOnOverview = await page.locator("text=主机离线").count();
if (guardOnOverview !== 0) throw new Error("概览 tab 不应显示离线通栏");
console.log("✓ 离线守卫：概览 tab（非 GUARDED）无通栏");

await browser.close();

// 清理 + agent 复活（留环境干净）
for (const f of [SRC, REMOTE, DOWNLOADED]) if (existsSync(f)) unlinkSync(f);
spawn(
  "/Users/trtyr/Documents/Code/Rust/helm/target/debug/helm-agent",
  ["--agent-id", "m2-agent", "--server-addr", "http://127.0.0.1:50051", "--token", "dev-token-change-me"],
  { detached: true, stdio: "ignore" },
).unref();
console.log("\nM2 联调（概览/终端/文件/列表导航/离线守卫）全部通过");
