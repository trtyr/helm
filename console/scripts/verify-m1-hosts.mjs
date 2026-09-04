/**
 * M1 主机列表联调验证（playwright）：
 * 创建(reverse+tags/forward+addr) → 列表 → 编辑 → 标签过滤 → 搜索 → Agents 视图 → 删除。
 */
import { chromium } from "playwright";

const BASE = process.env.M1_BASE ?? "http://localhost:5180";
const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

// 登录
await page.goto(`${BASE}/`);
await page.fill("#username", "admin");
await page.fill("#password", "admin123");
await page.click("button[type=submit]");
await page.waitForURL(/\/hosts$/);

// 1. 创建 reverse 主机（带标签）
await page.click("text=创建主机");
await page.fill("#hostname", "m1-test-web");
await page.fill("#tag-input", "m1");
await page.press("#tag-input", "Enter");
await page.fill("#tag-input", "web");
await page.press("#tag-input", "Enter");
await page.click("form button[type=submit]");
await page.waitForSelector("tr:has-text('m1-test-web')");
console.log("✓ 创建 reverse 主机（含 m1/web 标签）");

// 2. 创建 forward 主机（addr 条件显示验证）
await page.click("text=创建主机");
await page.fill("#hostname", "m1-test-fwd");
await page.click("text=正向（Server 拨号）");
await page.fill("#addr", "10.0.0.9:50052");
await page.click("form button[type=submit]");
await page.waitForSelector("tr:has-text('m1-test-fwd')");
console.log("✓ 创建 forward 主机（addr=10.0.0.9:50052，正向时 addr 字段显示）");

// 3. 编辑：改名
await page.hover("tr:has-text('m1-test-web')");
await page.click("tr:has-text('m1-test-web') >> text=编辑");
await page.fill("#hostname", "m1-test-renamed");
await page.click("form button[type=submit]");
await page.waitForSelector("tr:has-text('m1-test-renamed')");
console.log("✓ 编辑主机名 m1-test-web → m1-test-renamed");

// 4. 标签过滤
await page.selectOption("select[aria-label=标签过滤]", "m1");
await page.waitForSelector("tr:has-text('m1-test-renamed')");
const fwdVisible = await page.locator("tr:has-text('m1-test-fwd')").isVisible().catch(() => false);
if (fwdVisible) throw new Error("标签过滤失效：m1-test-fwd（无 m1 标签）不应显示");
console.log("✓ 标签过滤 m1：只剩带 m1 标签的主机");
await page.selectOption("select[aria-label=标签过滤]", "");

// 5. 搜索（前端过滤）
await page.fill("input[placeholder='搜索主机名']", "fwd");
await page.waitForSelector("tr:has-text('m1-test-fwd')");
const renamedVisible = await page.locator("tr:has-text('m1-test-renamed')").isVisible().catch(() => false);
if (renamedVisible) throw new Error("搜索过滤失效");
console.log("✓ 搜索 'fwd'：只剩 m1-test-fwd");
await page.fill("input[placeholder='搜索主机名']", "");

// 6. Agents 视图
await page.click("[data-testid=tab-agents]");
await page.waitForSelector("table");
console.log("✓ Agents 视图切换（空态或列表渲染正常）");
await page.click("[data-testid=tab-hosts]");

// 7. 删除（输入主机名二次确认）
for (const name of ["m1-test-fwd", "m1-test-renamed"]) {
  await page.hover(`tr:has-text('${name}')`);
  await page.click(`tr:has-text('${name}') >> text=删除`);
  await page.fill("div[role=dialog] input", name);
  await page.click("div[role=dialog] button:has-text('删除')");
  await page.waitForSelector(`tr:has-text('${name}')`, { state: "detached" });
}
console.log("✓ 删除两台测试主机（输入主机名确认 + 列表消失）");

// 8. 截图
await page.screenshot({ path: ".shots/m1-hosts.png", fullPage: true });
console.log("✓ 截图 .shots/m1-hosts.png");

await browser.close();
console.log("\nM1 主机 CRUD 联调全部通过");
