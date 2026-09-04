/** 格式化纯函数（组件无关）。 */

/** 相对时间（xx 前）。 */
export function relativeTime(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "—";
  const diffMs = now - new Date(iso).getTime();
  if (diffMs < 0) return "刚刚";
  const s = Math.floor(diffMs / 1000);
  if (s < 60) return `${s}s 前`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m 前`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h 前`;
  const d = Math.floor(h / 24);
  return `${d}d 前`;
}
