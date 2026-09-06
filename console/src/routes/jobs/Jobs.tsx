import { useMemo } from "react";
import { Link, useNavigate, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { relativeTime } from "../../lib/format";
import { jobStatusMeta } from "../../lib/job";
import { SkeletonRows } from "../../components/ui";

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
  const limit = 20;

  const jobsQuery = useQuery({
    queryKey: ["jobs", page],
    queryFn: async () => {
      const r = await api<{ jobs: Job[] }>(`/api/v1/jobs?page=${page}&limit=${limit}`);
      return { at: Date.now(), jobs: r.jobs ?? [] }; // now 在异步侧产生（render 纯度）
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

  // 服务端无 status/host 过滤参数 → 前端过滤（当前页内）
  const jobs = (jobsQuery.data?.jobs ?? []).filter((j) => {
    if (status !== "all" && j.status !== status) return false;
    if (hostFilter && j.host_id !== hostFilter) return false;
    return true;
  });
  const counts = useMemo(() => {
    const all = jobsQuery.data?.jobs ?? [];
    return {
      all: all.length,
      succeeded: all.filter((j) => j.status === "succeeded").length,
      failed: all.filter((j) => j.status === "failed").length,
      running: all.filter((j) => j.status === "running" || j.status === "queued").length,
    };
  }, [jobsQuery.data]);

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
              <th className="px-4 py-2.5 font-normal">主机</th>
              <th className="px-4 py-2.5 font-normal">命令</th>
              <th className="px-4 py-2.5 font-normal">状态</th>
              <th className="px-4 py-2.5 font-normal">退出码</th>
              <th className="px-4 py-2.5 font-normal">时间</th>
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
        <div className="flex h-12 items-center justify-between border-t border-gray-400 px-4 text-label-13 text-gray-900">
          <span>
            {status !== "all" || hostFilter
              ? `过滤后 ${jobs.length} 条 · 第 ${page} 页`
              : (jobsQuery.data?.jobs?.length ?? 0) > 0
                ? `第 ${page} 页 · 本页 ${jobsQuery.data?.jobs?.length} 条`
                : "无任务"}
          </span>
          <span className="flex items-center gap-1">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => patchParams({ page: String(page - 1) })}
              aria-label="上一页"
              className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
            >
              <ChevronLeft size={14} strokeWidth={1.5} />
            </button>
            <span className="font-mono">{page}</span>
            <button
              type="button"
              disabled={(jobsQuery.data?.jobs ?? []).length < limit}
              onClick={() => patchParams({ page: String(page + 1) })}
              aria-label="下一页"
              className="flex h-7 w-7 items-center justify-center rounded-md transition-colors duration-150 hover:bg-gray-200 disabled:opacity-30"
            >
              <ChevronRight size={14} strokeWidth={1.5} />
            </button>
          </span>
        </div>
      </div>
    </div>
  );
}
