/**
 * M1 登录流浏览器验证（playwright）：
 * 1. 未认证访问 / → 重定向 /login（守卫）
 * 2. 错误凭据 → 行内错误
 * 3. 正确凭据 → 跳 /hosts（redirect 参数生效）
 * 4. 截图（暗色登录页 / hosts 占位）供视觉检查
 */
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const BASE = process.env.M1_BASE ?? "http://localhost:5180";
mkdirSync(".shots", { recursive: true });

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

// 1. 守卫：未认证访问 / → /login?redirect=
await page.goto(`${BASE}/`);
await page.waitForURL(/\/login\?redirect=/);
console.log("✓ 守卫：/ → /login?redirect=" + new URL(page.url()).searchParams.get("redirect"));

// 2. 错误凭据 → 行内错误
await page.fill("#username", "admin");
await page.fill("#password", "wrong-pass");
await page.click("button[type=submit]");
await page.waitForSelector("text=用户名或密码错误");
console.log("✓ 错误凭据 → 行内错误提示");

// 3. 正确凭据 → 跳回原目标（/ → /hosts）
await page.fill("#password", "admin123");
await page.click("button[type=submit]");
await page.waitForURL(/\/hosts$/);
await page.waitForSelector("button:has-text('创建主机')");
console.log("✓ 登录成功 → /hosts（redirect 回跳 + token 落地 localStorage）");

// 3.5 已登录访问 /login → 直接重定向进应用（规格状态矩阵）
await page.goto(`${BASE}/login`);
await page.waitForURL(/\/hosts$/);
console.log("✓ 已登录访问 /login → 自动进入应用");

// 4. 截图（hosts 在 token 仍有效时先截；login 清 token 后截）
await page.goto(`${BASE}/hosts`);
await page.waitForSelector("button:has-text('创建主机')");
await page.screenshot({ path: ".shots/m1-hosts-placeholder.png", fullPage: true });

await page.goto(`${BASE}/login`);
await page.evaluate(() => localStorage.clear());
await page.reload();
await page.screenshot({ path: ".shots/m1-login-dark.png", fullPage: true });
console.log("✓ 截图已存 .shots/");

await browser.close();
console.log("\nM1 登录流验证全部通过");
