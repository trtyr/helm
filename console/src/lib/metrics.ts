/** 指标域纯函数：指标名映射、滚动窗口、范围推导。 */

export interface Point {
  ts: number; // unix ms
  value: number;
}

/** 主图四组指标（规格 metrics.md 指标名映射）。 */
export const MAIN_METRICS = ["cpu.usage", "mem.percent", "disk.usage", "net.rx_bytes"] as const;

export const METRIC_LABEL: Record<string, string> = {
  "cpu.usage": "CPU 使用率",
  "mem.percent": "内存",
  "disk.usage": "磁盘",
  "net.rx_bytes": "网络 ↓",
  "net.tx_bytes": "网络 ↑",
  "net.rx_bytes_rate": "下载速率",
  "net.tx_bytes_rate": "上传速率",
  "proc.count": "进程数",
};

/** 次要指标（折叠 sparkline 区）。 */
export const MINOR_METRICS = ["net.rx_bytes", "net.tx_bytes", "proc.count"] as const;

/** 范围 → 拉取条数（按 30s 采集密度推导，规格 metrics.md）。 */
export function limitForRange(range: "live" | "1h" | "6h" | "24h"): number {
  switch (range) {
    case "1h":
      return 120;
    case "6h":
      return 144;
    case "24h":
      return 288;
    default:
      return 60;
  }
}

/** 滚动窗口追加：push 尾 + 超窗 shift 头（保持时间升序；重复 ts 幂等跳过）。 */
export function pushWindow(window: readonly Point[], point: Point, max: number): Point[] {
  if (window.length > 0 && point.ts <= window[window.length - 1].ts) {
    return [...window]; // 乱序/重复：保守丢弃
  }
  const next = [...window, point];
  return next.length > max ? next.slice(next.length - max) : next;
}

/** GET /metrics 混合列表 → 按 name 分桶（时间升序；同名同 ts 去重保留最新）。 */
export function bucketByName(metrics: readonly { name?: string; value?: number; ts?: string }[]): Map<string, Point[]> {
  const buckets = new Map<string, Map<number, number>>();
  for (const m of metrics) {
    if (!m.name || m.value == null || !m.ts) continue;
    const ts = new Date(m.ts).getTime();
    const bucket = buckets.get(m.name) ?? new Map<number, number>();
    bucket.set(ts, m.value);
    buckets.set(m.name, bucket);
  }
  const out = new Map<string, Point[]>();
  for (const [name, tsMap] of buckets) {
    out.set(
      name,
      [...tsMap.entries()].sort((a, b) => a[0] - b[0]).map(([ts, value]) => ({ ts, value })),
    );
  }
  return out;
}

/** 字节速率格式化（网络图轴）。 */
export function bytesLabel(v: number): string {
  if (v >= 1024 ** 3) return `${(v / 1024 ** 3).toFixed(1)}G`;
  if (v >= 1024 ** 2) return `${(v / 1024 ** 2).toFixed(1)}M`;
  if (v >= 1024) return `${(v / 1024).toFixed(1)}K`;
  return `${Math.round(v)}`;
}
