/**
 * 剪贴板复制：兼容非安全上下文。
 *
 * 控制台经常经局域网 IP（http://192.168.x.x:5180）访问——非 HTTPS 非 localhost
 * 即非安全上下文，`navigator.clipboard` 为 undefined，不能直接用。
 * 策略：安全上下文优先 Clipboard API；否则降级 `document.execCommand("copy")
 * （已废弃但在 HTTP 下稳定可用）。返回是否成功，由调用方决定提示。
 */
export async function copyText(text: string): Promise<boolean> {
  if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      // 权限拒绝等 → 走降级
    }
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch {
    return false;
  }
}
