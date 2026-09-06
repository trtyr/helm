/**
 * M5 账号管理联调（playwright + 真后端）：
 * 登录页 notice 提示 → 设置页「账号」卡（单用户模式/身份/角色/创建时间）
 * → 改密错误路径（当前密码错）→ 改密成功 → 强制重登（notice）→ 新密登录 → 还原 admin123。
 * 前置：后端 18081 + dev 5180 已启动。
 */
import { chromium } from "playwright";

const BASE = "http://localhost:5180";

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

async function login(username, password) {
  await page.goto(`${BASE}/login`);
  await page.fill("#username", username);
  await page.fill("#password", password);
  await page.click("button[type=submit]");
}

// ── 1. 登录进入 ──
await login("admin", "admin123");
await page.waitForURL(/\/(hosts|dashboard)$/, { timeout: 90_000 });
console.log("✓ 登录 admin/admin123");

// ── 2. 设置页账号卡 ──
await page.goto(`${BASE}/settings`);
await page.waitForSelector("text=单用户模式", { timeout: 30_000 });
await page.waitForSelector("text=修改用户名", { timeout: 15_000 });
await page.waitForSelector("text=修改密码", { timeout: 15_000 });
const accountText = await page.textContent("section:nth-of-type(1)");
if (!accountText?.includes("admin（管理员）")) throw new Error("账号卡未显示角色 admin（管理员）");
if (!accountText?.includes("创建时间")) throw new Error("账号卡未显示创建时间");
console.log("✓ 设置：账号卡（单用户模式 / admin（管理员）/ 创建时间）");

// ── 3. 改密错误路径：当前密码错 → 行内「当前密码错误」 ──
const pwForm = page.locator("form", { hasText: "修改密码" });
await pwForm.locator("input[placeholder='当前密码']").fill("wrong-pass-0");
await pwForm.locator("input[placeholder='新密码（≥ 6 字符）']").fill("verify-pass-9");
await pwForm.locator("input[placeholder='确认新密码']").fill("verify-pass-9");
await pwForm.locator("button:has-text('更新')").click();
await page.waitForSelector("text=当前密码错误", { timeout: 30_000 });
console.log("✓ 改密：错误当前密码 → 行内「当前密码错误」");

// ── 4. 改密成功 → 强制重登 + notice ──
await pwForm.locator("input[placeholder='当前密码']").fill("admin123");
await pwForm.locator("button:has-text('更新')").click();
await page.waitForURL(/\/login\?notice=/, { timeout: 30_000 });
await page.waitForSelector("text=密码已更新，请重新登录", { timeout: 15_000 });
console.log("✓ 改密：成功 → 回登录页（notice 提示条）");

// ── 5. 新密码登录 → 还原 admin123 ──
await page.fill("#username", "admin");
await page.fill("#password", "verify-pass-9");
await page.click("button[type=submit]");
await page.waitForURL(/\/(hosts|dashboard)$/, { timeout: 90_000 });
console.log("✓ 新密码 verify-pass-9 登录成功");

await page.goto(`${BASE}/settings`);
const pwForm2 = page.locator("form", { hasText: "修改密码" });
await pwForm2.locator("input[placeholder='当前密码']").fill("verify-pass-9");
await pwForm2.locator("input[placeholder='新密码（≥ 6 字符）']").fill("admin123");
await pwForm2.locator("input[placeholder='确认新密码']").fill("admin123");
await pwForm2.locator("button:has-text('更新')").click();
await page.waitForURL(/\/login\?notice=/, { timeout: 30_000 });

// ── 6. 还原后 admin/admin123 可登录 ──
await login("admin", "admin123");
await page.waitForURL(/\/(hosts|dashboard)$/, { timeout: 90_000 });
console.log("✓ 凭据还原 admin/admin123 可登录");

await browser.close();
console.log("\nVERIFY M5 ACCOUNT: ALL CHECKS PASSED");
