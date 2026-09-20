import { useState } from "react";
import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { useWsStream } from "../api/ws";
import { relativeTime } from "../lib/format";

type HostView = components["schemas"]["HostView"];
type Alert = components["schemas"]["Alert"];
type Job = components["schemas"]["Job"];

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

/** 资源热点横向条形（roadmap T7：跨主机比大小才有意义）。 */
function TopBars({ title, data }: { title: string; data: { id: string; label: string; value: number }[] }) {
  const max = Math.max(...data.map((d) => d.value), 100);
  return (
    <div>
      <h3 className="text-label-13 text-gray-900">{title}</h3>
      <div className="mt-2 flex flex-col gap-1.5">
        {data.map((d) => (
          <div key={d.id} className="flex items-center gap-2">
            <Link
              to={`/hosts/${d.id}/overview`}
              className="w-28 shrink-0 truncate text-label-12 text-gray-900 hover:text-blue-1000"
              title={d.label}
            >
              {d.label}
            </Link>
            <div className="h-4 flex-1 overflow-hidden rounded bg-gray-200">
              <div
                className="h-full rounded bg-blue-1000/70"
                style={{ width: `${Math.min(100, (d.value / max) * 100)}%` }}
              />
            </div>
            <span className="w-12 shrink-0 text-right font-mono text-label-12 tabular-nums text-gray-1000">
              {d.value.toFixed(1)}%
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** 活动任务行（编号 + 命令 + 状态 + 时间，点击进详情）。 */
function TaskRow({ j, at }: { j: Job; at: number | undefined }) {
  const active = j.status === "running" || j.status === "queued";
  return (
    <Link
      to={j.id ? `/logs/jobs/${j.id}` : "/logs/jobs"}
      className="flex h-8 items-center gap-3 rounded px-2 text-label-13 transition-colors duration-150 hover:bg-gray-200"
    >
      <span className="w-16 shrink-0 font-mono text-label-12 text-gray-900">{j.id?.slice(0, 8) ?? "—"}</span>
      <span className="min-w-0 flex-1 truncate">
        {j.command}
        {(j.args ?? []).length > 0 ? ` ${(j.args ?? []).join(" ")}` : ""}
      </span>
      <span
        className={`w-16 shrink-0 font-mono text-label-12 ${
          active ? "text-blue-1000" : j.status === "succeeded" ? "text-green-1000" : "text-red-1000"
        }`}
      >
        {j.status}
      </span>
      <span className="shrink-0 font-mono text-label-12 text-gray-900">
        {relativeTime(j.started_at ?? undefined, at)}
      </span>
    </Link>
  );
}

/**
 * /dashboard 仪表盘（roadmap T7 四象限决议：①在线态势 ②需要关注 ③活动任务流 ④资源热点）。
 * 全局 CPU 均值已移除（单用户伪指标）；资源热点按主机横向对比（跨主机比大小才有意义）。
 */
export default function Dashboard() {
  // 在线主机最新 CPU/内存（WS metrics 按主机记录，决策 004：前端丢帧过滤）
  const [cpuMem, setCpuMem] = useState<Map<string, { cpu?: number; mem?: number }>>(new Map());

  const hostsQuery = useQuery({
    queryKey: ["hosts", 1, null],
    queryFn: async () => {
      const r = await api<{ hosts: HostView[] }>("/api/v1/hosts?page=1&limit=200");
      return { at: Date.now(), hosts: r.hosts ?? [] };
    },
    refetchInterval: 30_000,
  });
  const hosts = hostsQuery.data?.hosts ?? [];

  const jobsQuery = useQuery({
    queryKey: ["jobs", "dash"],
    queryFn: async () => {
      const r = await api<{ jobs: Job[] }>("/api/v1/jobs?page=1&limit=50");
      return { at: Date.now(), jobs: r.jobs ?? [] };
    },
    refetchInterval: (q) =>
      (q.state.data?.jobs ?? []).some((j) => j.status === "running" || j.status === "queued")
        ? 10_000
        : 30_000,
  });
  const jobs = jobsQuery.data?.jobs ?? [];

  const unreadQuery = useQuery({
    queryKey: ["notifications", "unread-count"],
    queryFn: () => api<{ count: number }>("/api/v1/notifications/unread-count"),
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

  // WS metrics：按主机分别记录最新 cpu.usage / mem.percent（资源热点 Top5 数据源）
  useWsStream("/api/v1/metrics/stream", (raw) => {
    try {
      const m = JSON.parse(raw) as Frame;
      if (!m.host_id || m.value == null || !m.ts) return;
      if (m.name !== "cpu.usage" && m.name !== "mem.percent") return;
      setCpuMem((prev) => {
        const cur = prev.get(m.host_id!) ?? {};
        const next = m.name === "cpu.usage" ? { ...cur, cpu: m.value } : { ...cur, mem: m.value };
        const map = new Map(prev);
        map.set(m.host_id!, next);
        return map;
      });
    } catch {
      // 非 JSON 帧忽略
    }
  });

  const online = hosts.filter((h) => h.online);
  const offlineHosts = hosts.filter((h) => !h.online);
  const stale = hosts.filter((h) => h.stale).length;
  const percent = hosts.length > 0 ? Math.round((online.length / hosts.length) * 100) : 0;
  const unread = unreadQuery.data?.count ?? 0;
  const alertCount = alertsQuery.data?.alerts.length ?? 0;
  const runningJobs = jobs.filter((j) => j.status === "running" || j.status === "queued");
  const finishedJobs = jobs.filter((j) => j.status !== "running" && j.status !== "queued");
  const failedJobs = finishedJobs.filter((j) => j.status === "failed" || j.status === "timed_out");
  const topBy = (key: "cpu" | "mem") =>
    online
      .map((h) => ({ id: h.id ?? "", label: h.hostname ?? "—", value: cpuMem.get(h.id ?? "")?.[key] }))
      .filter((d): d is { id: string; label: string; value: number } => d.value != null)
      .sort((a, b) => b.value - a.value)
      .slice(0, 5);
  const cpuTop = topBy("cpu");
  const memTop = topBy("mem");

  if (hostsQuery.isPending) {
    return (
      <div className="flex flex-col gap-6">
        <div className="h-8 w-40 animate-pulse rounded bg-gray-200" />
        <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <div key={i} className="h-48 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      </div>
    );
  }

  if (hosts.length === 0) {
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

  return (
    <div className="flex flex-col gap-6">
      <div>
        <h1 className="text-heading-32">概览</h1>
        <p className="mt-1 text-copy-13 text-gray-900">主机状态与活动总览</p>
      </div>

      {/* 四象限（roadmap T7 决议） */}
      <div className="grid grid-cols-1 gap-6 xl:grid-cols-2">
        {/* ① 在线态势：在线 x/y + 离线主机直达 */}
        <section className="rounded-lg border border-gray-400 p-6">
          <h2 className="text-heading-16">在线态势</h2>
          <div className="mt-4 flex items-center gap-6">
            <Ring percent={percent} online={online.length} total={hosts.length} />
            <div className="text-label-13 text-gray-900">
              <p>
                在线 {online.length} · 离线 {offlineHosts.length}
              </p>
              <p className="mt-1 text-label-12">stale {stale}（心跳超时未注销）</p>
            </div>
          </div>
          {offlineHosts.length > 0 && (
            <div className="mt-4 flex flex-col">
              <p className="text-label-12 text-gray-900">离线主机（点击直达）</p>
              {offlineHosts.slice(0, 6).map((h) => (
                <Link
                  key={h.id}
                  to={`/hosts/${h.id}/overview`}
                  className="flex h-8 items-center gap-3 rounded px-2 text-label-13 transition-colors duration-150 hover:bg-gray-200"
                >
                  <span className="h-1.5 w-1.5 rounded-full bg-gray-500" />
                  <span className="min-w-0 flex-1 truncate">{h.hostname}</span>
                  <span className="shrink-0 font-mono text-label-12 text-gray-900">
                    {relativeTime(h.last_seen, hostsQuery.data?.at)}
                  </span>
                </Link>
              ))}
              {offlineHosts.length > 6 && (
                <p className="mt-1 px-2 text-label-12 text-gray-900">等 {offlineHosts.length - 6} 台…</p>
              )}
            </div>
          )}
        </section>

        {/* ② 需要关注：未读告警数 + 最近失败/超时任务 */}
        <section className="rounded-lg border border-gray-400 p-6">
          <h2 className="text-heading-16">需要关注</h2>
          <div className="mt-4 flex items-baseline gap-3">
            <span className="font-mono text-[2rem] leading-none tabular-nums text-gray-1000">{unread}</span>
            <span className="text-label-13 text-gray-900">条未读通知</span>
            <Link to="/notifications" className="text-label-13 text-blue-1000 hover:underline">
              去处理 →
            </Link>
          </div>
          <p className="mt-1 text-label-12 text-gray-900">
            最近告警 {alertCount} 条 ·{" "}
            <Link to="/alerts" className="text-blue-1000 hover:underline">
              查看告警 →
            </Link>
          </p>
          <div className="mt-4 flex flex-col">
            <p className="text-label-12 text-gray-900">最近失败 / 超时</p>
            {failedJobs.length === 0 ? (
              <p className="py-3 text-center text-label-13 text-gray-900">近期无失败任务 ✓</p>
            ) : (
              failedJobs.slice(0, 5).map((j) => <TaskRow key={j.id} j={j} at={jobsQuery.data?.at} />)
            )}
          </div>
        </section>

        {/* ③ 活动任务流：running + 最近完成，点进详情 */}
        <section className="rounded-lg border border-gray-400 p-6">
          <div className="flex items-center justify-between">
            <h2 className="text-heading-16">活动任务流</h2>
            <Link to="/logs/jobs" className="text-label-13 text-blue-1000 hover:underline">
              全部 →
            </Link>
          </div>
          <div className="mt-3 flex flex-col">
            <p className="text-label-12 text-gray-900">进行中（{runningJobs.length}）</p>
            {runningJobs.length === 0 ? (
              <p className="py-3 text-center text-label-13 text-gray-900">当前无进行中任务</p>
            ) : (
              runningJobs.slice(0, 5).map((j) => <TaskRow key={j.id} j={j} at={jobsQuery.data?.at} />)
            )}
          </div>
          <div className="mt-4 flex flex-col">
            <p className="text-label-12 text-gray-900">最近完成</p>
            {finishedJobs.length === 0 ? (
              <p className="py-3 text-center text-label-13 text-gray-900">暂无完成记录</p>
            ) : (
              finishedJobs.slice(0, 6).map((j) => <TaskRow key={j.id} j={j} at={jobsQuery.data?.at} />)
            )}
          </div>
        </section>

        {/* ④ 资源热点：CPU / 内存 Top5 在线主机横向条形 */}
        <section className="rounded-lg border border-gray-400 p-6">
          <h2 className="text-heading-16">资源热点（在线主机 Top5）</h2>
          {cpuTop.length === 0 && memTop.length === 0 ? (
            <p className="py-8 text-center text-label-13 text-gray-900">
              暂无指标数据（等待主机心跳上报）
            </p>
          ) : (
            <div className="mt-4 flex flex-col gap-5">
              <TopBars title="CPU 使用率" data={cpuTop} />
              <TopBars title="内存占用" data={memTop} />
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
