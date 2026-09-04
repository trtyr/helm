import { useEffect, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { useWsStream } from "../api/ws";
import { useTheme } from "../hooks/useTheme";
import { kindDot, type NotificationItem, type NotificationKind } from "../lib/notificationStore";
import { relativeTime } from "../lib/format";
import { pushWindow, type Point } from "../lib/metrics";

type HostView = components["schemas"]["HostView"];
type Alert = components["schemas"]["Alert"];

interface Frame {
  host_id?: string;
  name?: string;
  value?: number;
  ts?: number;
}

/** 环形图 SVG（规格 dashboard.md：底环 gray-700、进度 green-1000、中心百分比）。 */
function Ring({ percent, online, total }: { percent: number; online: number; total: number }) {
  const r = 34;
  const c = 2 * Math.PI * r;
  return (
    <svg width="80" height="80" viewBox="0 0 80 80" role="img" aria-label={`在线率 ${percent}%`}>
      <circle cx="40" cy="40" r={r} fill="none" stroke="var(--ds-gray-700)" strokeWidth="8" />
      <circle
        cx="40"
        cy="40"
        r={r}
        fill="none"
        stroke="var(--ds-green-1000)"
        strokeWidth="8"
        strokeDasharray={`${(c * percent) / 100} ${c}`}
        strokeLinecap="butt"
        transform="rotate(-90 40 40)"
      />
      <text x="40" y="38" textAnchor="middle" className="fill-[var(--ds-gray-1000)] font-mono" fontSize="15">
        {percent}%
      </text>
      <text x="40" y="54" textAnchor="middle" className="fill-[var(--ds-gray-900)] font-mono" fontSize="10">
        {online}/{total}
      </text>
    </svg>
  );
}

/** /dashboard 仪表盘（规格 dashboard.md F06–F11：统计卡 + 环形 + sparkline + 最近通知/告警）。 */
export default function Dashboard() {
  const { theme } = useTheme();
  const [cpuWindow, setCpuWindow] = useState<Point[]>([]);
  // 在线主机集合（WS 过滤用；ref 保存避免重建回调）
  const onlineHostsRef = useRef<Set<string>>(new Set());

  const hostsQuery = useQuery({
    queryKey: ["hosts", 1, null],
    queryFn: async () => {
      const r = await api<{ hosts: HostView[] }>("/api/v1/hosts?page=1&limit=200");
      return { at: Date.now(), hosts: r.hosts ?? [] };
    },
    refetchInterval: 30_000,
  });
  const hosts = hostsQuery.data?.hosts ?? [];

  // WS metrics：聚合在线主机 cpu.usage 均值（决策 004：前端丢帧过滤）
  useWsStream("/api/v1/metrics/stream", (raw) => {
    try {
      const m = JSON.parse(raw) as Frame;
      if (m.name !== "cpu.usage" || m.value == null || !m.ts) return;
      const set = onlineHostsRef.current;
      if (set.size > 0 && !set.has(m.host_id ?? "")) return;
      setCpuWindow((prev) => pushWindow(prev, { ts: m.ts!, value: m.value! }, 60));
    } catch {
      // 非 JSON 帧忽略
    }
  });
  // 在线主机集合随 hosts 数据同步（依赖 query data 稳定引用）
  const hostsData = hostsQuery.data;
  useEffect(() => {
    const next = new Set<string>();
    (hostsData?.hosts ?? []).filter((h) => h.online).forEach((h) => next.add(h.id ?? ""));
    onlineHostsRef.current = next;
  }, [hostsData]);

  const notificationsQuery = useQuery({
    queryKey: ["notifications", "recent8"],
    queryFn: async () => {
      const r = await api<{ notifications: NotificationItem[] }>("/api/v1/notifications?page=1&limit=8");
      return { at: Date.now(), notifications: r.notifications ?? [] };
    },
    refetchInterval: 30_000,
  });
  const alertsQuery = useQuery({
    queryKey: ["alerts", "recent5"],
    queryFn: async () => {
      const r = await api<{ alerts: Alert[] }>("/api/v1/alerts?page=1&limit=5");
      return { at: Date.now(), alerts: r.alerts ?? [] };
    },
    refetchInterval: 30_000,
  });
  const unreadQuery = useQuery({
    queryKey: ["notifications", "unread-count"],
    queryFn: () => api<{ count: number }>("/api/v1/notifications/unread-count"),
    refetchInterval: 30_000,
  });

  const online = hosts.filter((h) => h.online).length;
  const stale = hosts.filter((h) => h.stale).length;
  const percent = hosts.length > 0 ? Math.round((online / hosts.length) * 100) : 0;
  const unread = unreadQuery.data?.count ?? 0;
  const alertCount = alertsQuery.data?.alerts.length ?? 0;
  const latestCpu = cpuWindow[cpuWindow.length - 1]?.value;

  if (hostsQuery.isPending) {
    return (
      <div className="flex flex-col gap-6">
        <div className="h-8 w-40 animate-pulse rounded bg-gray-200" />
        <div className="grid grid-cols-2 gap-6 xl:grid-cols-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-24 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      </div>
    );
  }

  if (!hostsQuery.isError && hosts.length === 0) {
    return (
      <div className="rounded-lg border border-dashed border-gray-500 p-16 text-center">
        <p className="text-copy-13 text-gray-900">还没有主机</p>
        <Link
          to="/hosts"
          className="mt-4 inline-block h-8 rounded-md border border-gray-500 px-4 text-label-13 leading-8 transition-colors duration-150 hover:bg-gray-200"
        >
          创建主机
        </Link>
      </div>
    );
  }

  const stats = [
    { label: "主机", value: hosts.length, unit: "台主机", to: "/hosts", dot: false },
    { label: "在线", value: online, unit: "台在线", to: "/hosts", dot: false },
    { label: "未读通知", value: unread, unit: "条未读", to: "/notifications", dot: unread > 0 },
    { label: "告警", value: alertCount, unit: "最近 5 条内", to: "/alerts", dot: false },
  ];

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-heading-32">概览</h1>
        <p className="mt-1 text-copy-13 text-gray-900">主机状态与活动总览</p>
      </div>

      {/* 统计卡 4 联 */}
      <div className="grid grid-cols-2 gap-6 xl:grid-cols-4">
        {stats.map((s) => (
          <Link
            key={s.label}
            to={s.to}
            className="relative flex h-24 flex-col justify-center rounded-lg border border-gray-400 px-6 transition-colors duration-150 hover:border-gray-500"
          >
            <span className="text-label-13 text-gray-900">{s.label}</span>
            <span className="mt-1 font-mono text-[2rem] leading-none tabular-nums text-gray-1000">
              {s.value}
            </span>
            <span className="mt-1 text-label-12 text-gray-900">{s.unit}</span>
            {s.dot && <span className="absolute right-4 top-4 h-2 w-2 rounded-full bg-blue-1000" />}
          </Link>
        ))}
      </div>

      {/* 环形 + sparkline */}
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        <div className="flex items-center gap-6 rounded-lg border border-gray-400 p-6 xl:col-span-5">
          <Ring percent={percent} online={online} total={hosts.length} />
          <div>
            <h2 className="text-heading-16">在线率</h2>
            <p className="mt-2 text-label-13 text-gray-900">
              离线 {hosts.length - online - stale} · stale {stale}
            </p>
          </div>
        </div>
        <div className="flex flex-col rounded-lg border border-gray-400 p-6 xl:col-span-7">
          <div className="flex items-baseline justify-between">
            <h2 className="text-heading-16">全局 CPU（实时）</h2>
            <span className="font-mono text-label-13 tabular-nums">
              {latestCpu != null ? `${latestCpu.toFixed(1)}%` : "等待数据"}
            </span>
          </div>
          <div className="mt-3 flex-1">
            <CpuSparkline points={cpuWindow} theme={theme} />
          </div>
          <p className="mt-1 text-label-12 text-gray-900">{online} 台在线均值 · 60 点窗口</p>
        </div>
      </div>

      {/* 最近通知 + 告警 */}
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-12">
        <div className="rounded-lg border border-gray-400 p-6 xl:col-span-5">
          <div className="flex items-center justify-between">
            <h2 className="text-heading-16">最近通知</h2>
            <Link to="/notifications" className="text-label-13 text-blue-1000 hover:underline">
              全部 →
            </Link>
          </div>
          <div className="mt-3 flex flex-col">
            {notificationsQuery.isPending ? (
              <p className="py-6 text-center text-label-13 text-gray-900">加载…</p>
            ) : (notificationsQuery.data?.notifications ?? []).length === 0 ? (
              <p className="py-6 text-center text-label-13 text-gray-900">暂无通知</p>
            ) : (
              (notificationsQuery.data?.notifications ?? []).map((n) => (
                <MiniNotification key={n.id} n={n} at={notificationsQuery.data?.at} />
              ))
            )}
          </div>
        </div>
        <div className="rounded-lg border border-gray-400 p-6 xl:col-span-7">
          <div className="flex items-center justify-between">
            <h2 className="text-heading-16">最近告警</h2>
            <Link to="/alerts" className="text-label-13 text-blue-1000 hover:underline">
              全部 →
            </Link>
          </div>
          <div className="mt-3 flex flex-col">
            {alertsQuery.isPending ? (
              <p className="py-6 text-center text-label-13 text-gray-900">加载…</p>
            ) : (alertsQuery.data?.alerts ?? []).length === 0 ? (
              <p className="py-6 text-center text-label-13 text-gray-900">暂无告警</p>
            ) : (
              (alertsQuery.data?.alerts ?? []).map((a) => {
                const h = hosts.find((x) => x.id === a.host_id);
                return (
                  <Link
                    key={a.id}
                    to={`/hosts/${a.host_id}/metrics?metric=${encodeURIComponent(a.metric_name ?? "")}`}
                    className="flex h-8 items-center gap-3 rounded px-2 text-label-13 transition-colors duration-150 hover:bg-gray-200"
                  >
                    <span className="w-16 shrink-0 truncate">{h?.hostname ?? "—"}</span>
                    <span className="w-28 shrink-0 font-mono text-gray-900">{a.metric_name}</span>
                    <span className="font-mono font-bold tabular-nums text-red-1000">
                      {(a.value ?? 0).toFixed(1)}
                    </span>
                    <span className="font-mono text-gray-900">→ {a.threshold}</span>
                    <span className="ml-auto font-mono text-label-12 text-gray-900">
                      {relativeTime(a.created_at, alertsQuery.data?.at)}
                    </span>
                  </Link>
                );
              })
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

function MiniNotification({ n, at }: { n: NotificationItem; at: number | undefined }) {
  const dot = kindDot(n.kind as NotificationKind);
  return (
    <div className="flex h-8 items-center gap-2.5 rounded px-2 transition-colors duration-150 hover:bg-gray-200">
      <span className={`text-label-13 ${dot.cls}`}>{dot.symbol}</span>
      <span className="min-w-0 flex-1 truncate text-label-13">{n.message}</span>
      <span className="shrink-0 font-mono text-label-12 text-gray-900">
        {relativeTime(n.created_at, at)}
      </span>
    </div>
  );
}

/** 全局 CPU sparkline（决策 006：SVG 自绘 polyline，非 uPlot）。 */
function CpuSparkline({ points, theme }: { points: Point[]; theme: string }) {
  if (points.length < 2) {
    return <div className="h-16" />;
  }
  const w = 560;
  const h = 64;
  const values = points.map((p) => p.value);
  const min = Math.min(...values, 0);
  const max = Math.max(...values, 100);
  const path = points
    .map((p, i) => {
      const x = (i / (points.length - 1)) * w;
      const y = h - ((p.value - min) / (max - min || 1)) * h;
      return `${i === 0 ? "M" : "L"}${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");
  const area = `${path} L${w},${h} L0,${h} Z`;
  void theme; // 主题变化触发重绘（CSS 变量自动生效）
  return (
    <svg viewBox={`0 0 ${w} ${h}`} className="h-16 w-full" preserveAspectRatio="none" aria-hidden>
      <path d={area} fill="var(--ds-blue-1000)" fillOpacity="0.08" />
      <path d={path} fill="none" stroke="var(--ds-blue-1000)" strokeWidth="1.5" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}
