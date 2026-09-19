import { useMemo } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { relativeTime } from "../../lib/format";
import { jobStatusMeta } from "../../lib/job";
import { SkeletonRows } from "../../components/ui";
import { SortableTh } from "../../components/tableControls";
import { PaginationBar } from "../../components/pagination";

type Job = components["schemas"]["Job"];
type Host = components["schemas"]["Host"];

const STATUS_FILTERS = ["all", "succeeded", "failed", "running"] as const;
const FILTER_LABEL: Record<string, string> = {
  all: "全部",
  succeeded: "成功",
  failed: "失败",
  running: "运行中",
};

/** /jobs 全局任务列表（规格 jobs.md F26：状态 chips + 主机过滤 + 分页）。 */
export default function Jobs() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const status = params.get("status") ?? "all";
  const hostFilter = params.get("host") ?? "";
  const page = Math.max(1, Number(params.get("page") ?? 1));
  const limit = Math.max(1, Number(params.get("limit") ?? 20));
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

  const jobsQuery = useQuery({
    queryKey: ["jobs", page, limit, status, hostFilter, sortField],
    queryFn: async () => {
      // D4：服务端过滤（status/host_id），不再前端筛当前页
      const sp = new URLSearchParams({ page: String(page), limit: String(limit) });
      if (status !== "all") sp.set("status", status);
      if (hostFilter) sp.set("host_id", hostFilter);
      if (sortField) sp.set("sort", sortField);
      const r = await api<{ jobs: Job[]; total?: number }>(`/api/v1/jobs?${sp}`);
      return { at: Date.now(), jobs: r.jobs ?? [], total: r.total ?? 0 }; // now 在异步侧产生（render 纯度）
    },
    refetchInterval: (q) =>
      (q.state.data?.jobs ?? []).some((j) => j.status === "running" || j.status === "queued") ? 10_000 : false,
  });
  const hostsQuery = useQuery({
    queryKey: ["hosts", "all"],
    queryFn: () => api<{ hosts: Host[] }>("/api/v1/hosts?page=1&limit=200"),
  });

  const hostName = useMemo(() => {
    const map = new Map((hostsQuery.data?.hosts ?? []).map((h) => [h.id ?? "", h.hostname ?? h.id ?? ""]));
    return (id: string | undefined) => map.get(id ?? "") ?? "—";
  }, [hostsQuery.data]);

  // D4：过滤在服务端完成，直接渲染返回集（useMemo 防每渲染新数组触发下游 useMemo 重算）
  const jobs = useMemo(() => jobsQuery.data?.jobs ?? [], [jobsQuery.data]);
  // 统计卡片：无过滤时即全量；有过滤时统计的是过滤后集合（语义：当前视图分布）
  const counts = useMemo(() => {
    const all = jobs;
    return {
      all: all.length,
      succeeded: all.filter((j) => j.status === "succeeded").length,
      failed: all.filter((j) => j.status === "failed").length,
      running: all.filter((j) => j.status === "running" || j.status === "queued").length,
    };
  }, [jobs]);

  function patchParams(patch: Record<string, string | null>) {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v === null || v === "" || v === "all") next.delete(k);
      else next.set(k, v);
    }
    if (!("page" in patch)) next.delete("page"); // 过滤变化回第一页
    setParams(next, { replace: true });
  }

  return (
    <div className="flex flex-col gap-5">
      <p className="-mt-1 text-copy-13 text-gray-900">全部主机的命令执行历史</p>

      {/* 列表卡 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        {/* 工具行：状态 chips + 主机过滤 */}
        <div className="flex flex-wrap items-center gap-2 border-b border-gray-400 px-4 py-2">
          {STATUS_FILTERS.map((s) => (
            <button
              key={s}
              type="button"
              onClick={() => patchParams({ status: s })}
              aria-pressed={status === s}
              className={`h-7 rounded-full border px-3 text-label-12 transition-colors duration-150 ${
                status === s
                  ? "border-gray-1000 bg-gray-200 text-gray-1000"
                  : "border-gray-500 text-gray-900 hover:border-gray-600"
              }`}
            >
              {FILTER_LABEL[s]} {counts[s as keyof typeof counts]}
            </button>
          ))}
          <select
            aria-label="按主机过滤"
            value={hostFilter}
            onChange={(e) => patchParams({ host: e.target.value })}
            disabled={hostsQuery.isError}
            className="ml-auto h-8 rounded-md border border-gray-400 bg-gray-100 px-2 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 disabled:opacity-40"
          >
            <option value="">全部主机</option>
            {(hostsQuery.data?.hosts ?? []).map((h) => (
              <option key={h.id} value={h.id ?? ""}>
                {h.hostname}
              </option>
            ))}
          </select>
        </div>

        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-20 px-4 py-2.5 font-normal">编号</th>
              <SortableTh className="px-4" label="主机" sortKey="host_id" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4" label="命令" sortKey="command" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4" label="状态" sortKey="status" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4" label="退出码" sortKey="exit_code" sort={sort} onSort={toggleSort} />
              <SortableTh className="px-4" label="时间" sortKey="started_at" sort={sort} onSort={toggleSort} />
            </tr>
          </thead>
          <tbody>
            {jobsQuery.isPending ? (
              <SkeletonRows rows={8} cols={6} />
            ) : jobsQuery.isError ? (
              <tr>
                <td colSpan={6} className="px-4 py-12 text-center">
                  <span className="text-label-13 text-red-1000">查询失败：{(jobsQuery.error as Error).message}</span>
                  <button type="button" onClick={() => jobsQuery.refetch()} className="ml-3 text-label-13 text-blue-1000 hover:underline">
                    重试
                  </button>
                </td>
              </tr>
            ) : jobs.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-12 text-center text-label-13 text-gray-900">
                  {(jobsQuery.data?.jobs ?? []).length === 0 ? (
                    <>
                      还没有任务历史 ·{" "}
                      <Link to="/hosts" className="text-blue-1000 hover:underline">
                        去主机执行命令
                      </Link>
                    </>
                  ) : (
                    <>
                      没有匹配的任务 ·{" "}
                      <button type="button" onClick={() => patchParams({ status: null, host: null })} className="text-blue-1000 hover:underline">
                        清除过滤
                      </button>
                    </>
                  )}
                </td>
              </tr>
            ) : (
              jobs.map((job) => {
                const meta = jobStatusMeta(job.status);
                return (
                  <tr
                    key={job.id}
                    onClick={() => navigate(`/jobs/${job.id}`)}
                    className="cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-4 py-3 font-mono text-label-13 text-gray-900" title={job.id}>
                      #{job.id?.slice(0, 4)}
                    </td>
                    <td className="px-4 py-3 text-label-13">{hostName(job.host_id)}</td>
                    <td className="max-w-64 px-4 py-3">
                      <span className="block truncate font-mono text-label-13 text-gray-900" title={[job.command, ...(job.args ?? [])].join(" ")}>
                        {job.command} {(job.args ?? []).join(" ")}
                      </span>
                    </td>
                    <td className={`px-4 py-3 text-label-13 ${meta.cls}`}>
                      {meta.label}
                      {job.status === "running" && (
                        <span className="ml-2 inline-block h-3 w-3 animate-spin rounded-full border border-blue-1000 border-t-transparent align-[-1px]" />
                      )}
                    </td>
                    <td
                      className={`px-4 py-3 font-mono text-label-13 tabular-nums ${
                        job.exit_code == null ? "text-gray-900" : job.exit_code === 0 ? "text-green-1000" : "text-red-1000"
                      }`}
                    >
                      {job.exit_code ?? "—"}
                    </td>
                    <td className="px-4 py-3 font-mono text-label-13 text-gray-900">
                      {relativeTime(job.finished_at ?? job.started_at, jobsQuery.data?.at)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>

        {/* 底栏分页 */}
        <PaginationBar page={page} limit={limit} total={jobsQuery.data?.total ?? 0} onPatch={patchParams} />
      </div>
    </div>
  );
}
