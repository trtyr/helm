import { useEffect, useMemo, useRef, useState } from "react";
import { useOutletContext, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { useWsStream } from "../../api/ws";
import TimeSeriesChart from "../../components/TimeSeriesChart";
import { useTheme } from "../../hooks/useTheme";
import { METRIC_LABEL, MINOR_METRICS, bucketByName, limitForRange, pushWindow, type Point } from "../../lib/metrics";

type HostView = components["schemas"]["HostView"];
type Metric = components["schemas"]["Metric"];

interface Ctx {
  host: HostView;
}

type Range = "live" | "1h" | "6h" | "24h";
const RANGES: { value: Range; label: string }[] = [
  { value: "live", label: "实时" },
  { value: "1h", label: "1h" },
  { value: "6h", label: "6h" },
  { value: "24h", label: "24h" },
];

function readColor(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#0070f3";
}

/** CSS 变量解析（图表库需具体色值；tokens 单一来源；theme 变化时 render 派生重算）。 */
function useDsColor(name: string, theme: string): string {
  const [state, setState] = useState(() => ({ theme, color: readColor(name) }));
  if (state.theme !== theme) {
    // render 期间派生重算（React 认可的模式：条件 setState 立即重渲染，无级联）
    setState({ theme, color: readColor(name) });
  }
  return state.color;
}

/** sparkline（决策 006：SVG 自绘无轴 polyline，非 uPlot）。 */
function Sparkline({ points, color }: { points: Point[]; color: string }) {
  if (points.length < 2) {
    return <div className="h-8" />;
  }
  const w = 200;
  const h = 32;
  const values = points.map((p) => p.value);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const path = points
    .map((p, i) => {
      const x = (i / (points.length - 1)) * w;
      const y = h - 3 - ((p.value - min) / span) * (h - 6);
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-8 w-full" preserveAspectRatio="none" aria-hidden>
      <path d={path} fill="none" stroke={color} strokeWidth={1.5} />
    </svg>
  );
}

/** /hosts/:id/metrics（规格 metrics.md F50–F53：2x2 主图 + WS 实时 + 范围切换 + 次要 sparkline）。 */
export default function Metrics() {
  const { host } = useOutletContext<Ctx>();
  const [searchParams] = useSearchParams();
  // F56：告警页跳转带 ?metric=cpu.usage → 对应卡高亮 200ms + 滚入视口（规格 alerts.md）
  const highlightMetric = searchParams.get("metric");
  const { theme } = useTheme();
  const [range, setRange] = useState<Range>("live");
  const [liveBuckets, setLiveBuckets] = useState<Map<string, Point[]>>(new Map());
  const [lastUpdate, setLastUpdate] = useState<number | null>(null);
  const [showMinor, setShowMinor] = useState(
    () => localStorage.getItem("helm-console.metrics-minor") === "1",
  );
  const blue = useDsColor("--ds-blue-1000", theme);
  const teal = useDsColor("--ds-teal-1000", theme);

  // 实时范围：连全局流，按 host 过滤（决策 004：后端无订阅协议，前端丢帧）
  const live = range === "live";
  useWsStream(
    "/api/v1/metrics/stream",
    (raw) => {
      try {
        const m = JSON.parse(raw) as { host_id?: string; name?: string; value?: number; ts?: number };
        if (m.host_id !== host.id || !m.name || m.value == null || !m.ts) return;
        setLiveBuckets((prev) => {
          const next = new Map(prev);
          next.set(m.name!, pushWindow(next.get(m.name!) ?? [], { ts: m.ts!, value: m.value! }, 60));
          return next;
        });
        setLastUpdate(m.ts ?? null);
      } catch {
        // 非 JSON 帧：忽略
      }
    },
    { enabled: live },
  );

  // 历史范围：静态查询
  const historyQuery = useQuery({
    queryKey: ["metrics", host.id, range],
    queryFn: async () => {
      const r = await api<{ metrics: Metric[] }>(
        `/api/v1/metrics?host_id=${host.id}&limit=${limitForRange(range)}`,
      );
      return { at: Date.now(), buckets: bucketByName(r.metrics ?? []) };
    },
    enabled: !live,
  });

  const buckets = live ? liveBuckets : historyQuery.data?.buckets ?? new Map<string, Point[]>();
  const updatedAt = live ? lastUpdate : historyQuery.data?.at ?? null;

  const get = (name: string): Point[] => buckets.get(name) ?? [];

  const netSeries = useMemo(
    () => [
      { label: "下载", points: get("net.rx_bytes"), color: blue, fillArea: true },
      { label: "上传", points: get("net.tx_bytes"), color: teal },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps -- buckets 引用已涵盖数据变化
    [buckets, blue, teal],
  );

  const cards = [
    { key: "cpu", title: METRIC_LABEL["cpu.usage"]!, series: [{ label: "CPU", points: get("cpu.usage"), color: blue, fillArea: true }], threshold: 90, yMax: 100, unit: "%", metric: "cpu.usage" },
    { key: "mem", title: METRIC_LABEL["mem.percent"]!, series: [{ label: "内存", points: get("mem.percent"), color: blue, fillArea: true }], threshold: 90, yMax: 100, unit: "%", metric: "mem.percent" },
    { key: "disk", title: METRIC_LABEL["disk.usage"]!, series: [{ label: "磁盘", points: get("disk.usage"), color: blue, fillArea: true }], threshold: 90, yMax: 100, unit: "%", metric: "disk.usage" },
    { key: "net", title: "网络", series: netSeries, threshold: undefined, yMax: undefined, unit: "", metric: "net.rx_bytes" },
  ];

  // F56 高亮：?metric= 匹配卡 ring 200ms + 滚入视口
  const highlightKey = cards.find((c) => c.metric === highlightMetric)?.key;
  const highlightRef = useRef<HTMLDivElement>(null);
  const [ring, setRing] = useState(false);
  useEffect(() => {
    if (highlightKey && highlightRef.current) {
      highlightRef.current.scrollIntoView({ behavior: "smooth", block: "center" });
      setRing(true);
      const t = setTimeout(() => setRing(false), 200);
      return () => clearTimeout(t);
    }
  }, [highlightKey]);

  const anyData = [...buckets.values()].some((b) => b.length > 0);
  const loading = live ? false : historyQuery.isPending;

  function toggleMinor() {
    setShowMinor((v) => {
      localStorage.setItem("helm-console.metrics-minor", v ? "0" : "1");
      return !v;
    });
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex rounded-md border border-gray-500 p-0.5">
          {RANGES.map((r) => (
            <button
              key={r.value}
              type="button"
              onClick={() => setRange(r.value)}
              aria-pressed={range === r.value}
              className={`h-7 rounded px-3 text-label-13 transition-colors duration-150 ${
                range === r.value ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              }`}
            >
              {r.label}
            </button>
          ))}
        </div>
        <span className="font-mono text-label-12 text-gray-900">
          {updatedAt ? `最后更新 ${new Date(updatedAt).toLocaleTimeString()}` : "等待数据"}
          {live && " · 实时已连接 ●"}
        </span>
      </div>

      {loading ? (
        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-[220px] animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      ) : !anyData ? (
        <div className="rounded-lg border border-dashed border-gray-500 p-12 text-center text-label-13 text-gray-900">
          暂无上报数据——Agent 每 30s 上报一批
        </div>
      ) : historyQuery.isError && !live ? (
        <div className="rounded-lg border border-gray-400 p-8 text-center">
          <p className="text-label-13 text-red-1000">查询失败：{(historyQuery.error as Error).message}</p>
          <button
            type="button"
            onClick={() => historyQuery.refetch()}
            className="mt-3 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200"
          >
            重试
          </button>
        </div>
      ) : (
        <>
          <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
            {cards.map((c) => {
              const has = c.series.some((s) => s.points.length > 0);
              const isHi = c.key === highlightKey;
              return (
                <div
                  key={c.key}
                  ref={isHi ? highlightRef : undefined}
                  className={`rounded-lg border p-6 transition-shadow duration-200 ${
                    isHi && ring ? "ring-2 ring-blue-1000" : ""
                  } border-gray-400`}
                >
                  <h3 className="text-label-14">{c.title}</h3>
                  <div className="mt-3">
                    {has ? (
                      <TimeSeriesChart
                        series={c.series}
                        threshold={c.threshold}
                        yMax={c.yMax}
                        unit={c.unit}
                        height={160}
                      />
                    ) : (
                      <div className="flex h-40 items-center justify-center text-label-12 text-gray-900">
                        暂无该指标数据
                      </div>
                    )}
                  </div>
                </div>
              );
            })}
          </div>

          <button
            type="button"
            onClick={toggleMinor}
            aria-expanded={showMinor}
            className="self-start text-label-13 text-gray-900 transition-colors duration-150 hover:text-gray-1000"
          >
            {showMinor ? "▾" : "▸"} 次要指标
          </button>
          {showMinor && (
            <div className="flex flex-col gap-4">
              {MINOR_METRICS.map((name) => {
                const points = get(name);
                const tail = points[points.length - 1]?.value;
                return (
                  <div key={name} className="flex items-center gap-4 rounded-lg border border-gray-400 px-4 py-3">
                    <span className="w-36 shrink-0 font-mono text-label-13 text-gray-900">{name}</span>
                    <Sparkline points={points} color={blue} />
                    <span className="w-20 shrink-0 text-right font-mono text-label-13 tabular-nums">
                      {tail != null ? tail.toFixed(1) : "—"}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </>
      )}
    </div>
  );
}
