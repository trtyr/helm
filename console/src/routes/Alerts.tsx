import { Link, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Info } from "lucide-react";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { relativeTime } from "../lib/format";
import { SortableTh, TableToolbar } from "../components/tableControls";

type Alert = components["schemas"]["Alert"];
type Host = components["schemas"]["Host"];

/** /alerts 告警（规格 alerts.md F54–F56：阈值说明条 + 列表 + 主机跳转）。 */
export default function Alerts() {
  const [params, setParams] = useSearchParams();
  const page = Math.max(1, Number(params.get("page") ?? 1));
  const limit = 20;
  // 列表基座（P001-T1c）：排序走服务端（sort 参数），状态存 URL params
  const sortField = params.get("sort") ?? "";
  const sortDesc = sortField.endsWith(":desc");
  const sortKey = sortField.replace(":desc", "");
  const sort: { key: string; desc: boolean } | null = sortKey ? { key: sortKey, desc: sortDesc } : null;
  const toggleSort = (key: string) => {
    const next = new URLSearchParams(params);
    if (sort?.key === key) {
      if (sort.desc) next.delete("sort");
      else next.set("sort", `${key}:desc`);
    } else next.set("sort", key);
    setParams(next, { replace: true });
  };

  // 列表基座（P001-T1）：搜索/级别筛选走服务端（q/level 参数），状态存 URL params
  const q = params.get("q") ?? "";
  const levelFilter = params.get("level") ?? "";
  const patchFilter = (patch: Record<string, string | null>) => {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v === null || v === "") next.delete(k);
      else next.set(k, v);
    }
    next.delete("page"); // 筛选变化回第一页
    setParams(next, { replace: true });
  };

  const alertsQuery = useQuery({
    queryKey: ["alerts", page, sortField, q, levelFilter],
    queryFn: async () => {
      const sp = new URLSearchParams({ page: String(page), limit: String(limit) });
      if (sortField) sp.set("sort", sortField);
      if (q) sp.set("q", q);
      if (levelFilter) sp.set("level", levelFilter);
      const r = await api<{ alerts: Alert[] }>(`/api/v1/alerts?${sp}`);
      return { at: Date.now(), alerts: r.alerts ?? [] };
    },
    refetchInterval: 30_000,
  });
  const hostsQuery = useQuery({
    queryKey: ["hosts", "all"],
    queryFn: () => api<{ hosts: Host[] }>("/api/v1/hosts?page=1&limit=200"),
  });
  const host = (id: string | undefined) =>
    (hostsQuery.data?.hosts ?? []).find((h) => h.id === id);

  function goPage(p: number) {
    const next = new URLSearchParams(params);
    next.set("page", String(p));
    setParams(next, { replace: true });
  }

  const alerts = alertsQuery.data?.alerts ?? [];

  return (
    <div className="flex flex-col gap-6">
      <p className="-mt-1 text-copy-13 text-gray-900">指标超过阈值的预警记录（保留 30 天）</p>

      <div className="flex items-center gap-2 rounded-lg bg-gray-200 px-4 py-2.5 text-label-13 text-gray-1000">
        <Info size={14} strokeWidth={1.5} className="shrink-0 text-blue-1000" />
        当前阈值固定：cpu.usage / mem.percent / disk.usage &gt; 90%（后端暂无配置接口）
      </div>

      {/* 工具行（列表基座：搜索 + 级别枚举下拉，筛选走服务端 q/level） */}
      <TableToolbar
        search={q}
        onSearch={(v) => patchFilter({ q: v })}
        filters={[
          {
            key: "level",
            label: "级别",
            value: levelFilter,
            options: [{ value: "warning", label: "warning" }],
            onChange: (v) => patchFilter({ level: v }),
          },
        ]}
      />

      <div className="overflow-hidden rounded-lg border border-gray-400">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <SortableTh className="px-4 py-2" label="主机" sortKey="host_id" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2" label="指标" sortKey="metric_name" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2" label="实测 → 阈值" sortKey="value" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2" label="级别" sortKey="level" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4 py-2" label="时间" sortKey="created_at" sort={sort} onSort={toggleSort} />
            </tr>
          </thead>
          <tbody>
            {alertsQuery.isPending ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载告警…
                </td>
              </tr>
            ) : alertsQuery.isError ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center">
                  <span className="text-label-13 text-red-1000">
                    查询失败：{(alertsQuery.error as Error).message}
                  </span>
                  <button
                    type="button"
                    onClick={() => alertsQuery.refetch()}
                    className="ml-3 text-label-13 text-blue-1000 hover:underline"
                  >
                    重试
                  </button>
                </td>
              </tr>
            ) : alerts.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center text-label-13 text-green-1000">
                  没有告警记录——一切正常 ✓
                </td>
              </tr>
            ) : (
              alerts.map((a) => {
                const h = host(a.host_id);
                return (
                  <tr
                    key={a.id}
                    className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-4 py-2.5 text-label-13">
                      {h && (
                        <Link
                          to={`/hosts/${h.id}/metrics?metric=${encodeURIComponent(a.metric_name ?? "")}`}
                          className="text-blue-1000 hover:underline"
                        >
                          {h.hostname}
                        </Link>
                      )}
                    </td>
                    <td className="px-4 py-2.5 font-mono text-label-13 text-gray-1000">{a.metric_name}</td>
                    <td className="px-4 py-2.5 font-mono text-label-13 tabular-nums">
                      <span className="font-bold text-red-1000">{(a.value ?? 0).toFixed(1)}</span>
                      <span className="text-gray-900"> → </span>
                      <span className="text-gray-1000">{a.threshold}</span>
                    </td>
                    <td className="px-4 py-2.5">
                      <span className="rounded bg-amber-1000/15 px-1.5 py-0.5 font-mono text-label-12 text-amber-1000">
                        {a.level ?? "warning"}
                      </span>
                    </td>
                    <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900">
                      {relativeTime(a.created_at, alertsQuery.data?.at)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>{alerts.length > 0 ? `第 ${page} 页` : ""}</span>
          <span className="flex items-center gap-3">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => goPage(page - 1)}
              className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
            >
              ‹ 上一页
            </button>
            <button
              type="button"
              disabled={alerts.length < limit}
              onClick={() => goPage(page + 1)}
              className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
            >
              下一页 ›
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}
