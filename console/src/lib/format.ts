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

/** 本地绝对时间（YYYY-MM-DD HH:mm:ss）。 */
export function formatDateTime(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "—";
  const pad = (n: number) => String(n).padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

/** 时长（两个 ISO 时间差，秒 < 60 显示 s，否则 m/s 组合）。 */
export function formatDuration(startIso: string | null | undefined, endIso: string | null | undefined): string {
  if (!startIso) return "—";
  const start = new Date(startIso).getTime();
  const end = endIso ? new Date(endIso).getTime() : Date.now();
  const s = Math.max(0, Math.round((end - start) / 1000));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  const rest = s % 60;
  if (m < 60) return `${m}m ${rest}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

/** unix 秒 → 启动至今时长（进程用）。 */
export function formatUptime(unixSecs: number | undefined, now = Date.now()): string {
  if (!unixSecs) return "—";
  const s = Math.max(0, Math.floor(now / 1000 - unixSecs));
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ${m % 60}m`;
  return `${Math.floor(h / 24)}d ${h % 24}h`;
}
