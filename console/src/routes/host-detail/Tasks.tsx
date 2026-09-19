import { useMemo, useState } from "react";
import { useNavigate, useOutletContext, useSearchParams } from "react-router-dom";
import { useMutation, useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { formatDateTime, formatDuration, relativeTime } from "../../lib/format";
import { toast } from "../../lib/toast";
import { intervalLabel, jobStatusMeta } from "../../lib/job";
import { useTableControls } from "../../lib/useTableControls";
import { SortableTh } from "../../components/tableControls";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type Job = components["schemas"]["Job"];

interface Ctx {
  host: HostView;
}

const STATUS_FILTERS = ["all", "succeeded", "failed", "running"] as const;
const FILTER_LABEL: Record<string, string> = { all: "全部", succeeded: "成功", failed: "失败", running: "运行中" };

/** /hosts/:id/tasks（Process Monitor 风格事件列表：绝对时间 + 完整命令行 + 退出码 + 耗时）。 */
export default function Tasks() {
  const { host } = useOutletContext<Ctx>();
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const status = params.get("status") ?? "all";
  const search = params.get("q") ?? "";
  const [drawerNonce, setDrawerNonce] = useState(0);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const jobsQuery = useQuery({
    queryKey: ["jobs", "host", host.id],
    queryFn: async () => {
      const r = await api<{ jobs: Job[] }>("/api/v1/jobs?page=1&limit=100");
      return { at: Date.now(), jobs: (r.jobs ?? []).filter((j) => j.host_id === host.id) };
    },
    refetchInterval: (q) =>
      (q.state.data?.jobs ?? []).some((j) => j.status === "running" || j.status === "queued") ? 10_000 : false,
  });

  // 定时任务聚合：同 task_id 多条 job 视为定时任务（前端推导）
  const scheduled = useMemo(() => {
    const byTask = new Map<string, Job[]>();
    for (const j of jobsQuery.data?.jobs ?? []) {
      if (!j.task_id) continue;
      const list = byTask.get(j.task_id) ?? [];
      list.push(j);
      byTask.set(j.task_id, list);
    }
    return [...byTask.entries()].map(([taskId, jobs]) => ({
      taskId,
      command: jobs[0]?.command ?? "",
      count: jobs.length,
      last: jobs[0], // 时间倒序第一条
    }));
  }, [jobsQuery.data]);

  const jobs = (jobsQuery.data?.jobs ?? []).filter((j) => {
    if (status === "all") return true;
    if (status === "running") return j.status === "running" || j.status === "queued";
    return j.status === status;
  });
  const shown = jobs.filter((j) => {
    if (!search.trim()) return true;
    const q = search.trim().toLowerCase();
    const cmdline = [j.command, ...(j.args ?? [])].join(" ").toLowerCase();
    return cmdline.includes(q) || (j.id ?? "").toLowerCase().includes(q);
  });

  // 列表基座（P001-T1）：排序层（状态枚举/搜索沿用页面既有实现）
  const { sort: jobSort, toggleSort: jobToggleSort, visible: jobVisible } = useTableControls(shown, {
    columns: [
      { key: "started_at", value: (j) => j.started_at ?? "" },
      { key: "cmdline", value: (j) => (j.command ?? "") + (j.args ?? []).join(" ") },
      { key: "kind", value: (j) => (j.task_id ? "定时" : "快速") },
      { key: "status", value: (j) => j.status ?? "" },
      { key: "exit_code", value: (j) => (j.exit_code == null ? null : j.exit_code) },
      {
        key: "duration",
        value: (j) =>
          j.started_at && j.finished_at
            ? new Date(j.finished_at).getTime() - new Date(j.started_at).getTime()
            : null,
      },
    ],
  });

  const offline = !host.online;

  function patchParams(patch: Record<string, string | null>) {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (v === null || v === "") next.delete(k);
      else next.set(k, v);
    }
    setParams(next, { replace: true });
  }

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-center justify-between">
        <h2 className="text-label-13 text-gray-900">任务</h2>
        <span className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => jobsQuery.refetch()}
            aria-label="刷新"
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <RefreshCw size={14} strokeWidth={1.5} />
          </button>
          <button
            type="button"
            onClick={() => setDrawerNonce((n) => n + 1)}
            disabled={offline || !agent}
            className="flex h-8 items-center gap-1.5 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
          >
            + 定时任务
          </button>
        </span>
      </div>

      {scheduled.length > 0 && (
        <div className="rounded-lg border border-gray-400 p-4">
          <p className="text-label-13 text-gray-900">定时任务（当前主机）</p>
          <div className="mt-3 flex flex-col gap-2">
            {scheduled.map((t) => {
              const meta = jobStatusMeta(t.last?.status);
              return (
                <div key={t.taskId} className="flex items-center gap-3 text-label-13">
                  <span className="text-gray-900">⏱</span>
                  <span className="min-w-0 flex-1 truncate font-mono" title={t.command}>
                    {t.command}
                  </span>
                  <span className="text-gray-900">{t.count} 次执行</span>
                  <span className={meta.cls}>{meta.label}</span>
                </div>
              );
            })}
          </div>
          <p className="mt-3 text-label-12 text-gray-900">Server 重启后自动恢复调度</p>
        </div>
      )}

      {/* 工具行：状态 chips + 搜索 */}
      <div className="flex flex-wrap items-center gap-2 border-b border-gray-400 pb-3">
        {STATUS_FILTERS.map((s) => (
          <button
            key={s}
            type="button"
            onClick={() => patchParams({ status: s })}
            aria-pressed={status === s}
            className={`h-7 rounded-full border px-3 font-mono text-label-12 transition-colors duration-150 ${
              status === s ? "border-gray-1000 bg-gray-200 text-gray-1000" : "border-gray-500 text-gray-900 hover:border-gray-600"
            }`}
          >
            {FILTER_LABEL[s]}
          </button>
        ))}
        <input
          value={search}
          onChange={(e) => patchParams({ q: e.target.value })}
          placeholder="搜索命令行 / 任务 ID"
          className="ml-auto h-8 w-56 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
      </div>

      {/* 事件列表：Process Monitor 式全量可见 */}
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <SortableTh className="w-44 px-4" label="时间" sortKey="started_at" sort={jobSort} onSort={jobToggleSort} />
              <SortableTh className="px-4" label="命令行" sortKey="cmdline" sort={jobSort} onSort={jobToggleSort} />
              <SortableTh className="w-16 px-4" label="类型" sortKey="kind" sort={jobSort} onSort={jobToggleSort} />
              <SortableTh className="w-20 px-4" label="状态" sortKey="status" sort={jobSort} onSort={jobToggleSort} />
              <SortableTh className="w-16 px-4" label="退出码" sortKey="exit_code" sort={jobSort} onSort={jobToggleSort} align="text-right" />
              <SortableTh className="w-20 px-4" label="耗时" sortKey="duration" sort={jobSort} onSort={jobToggleSort} align="text-right" />
            </tr>
          </thead>
          <tbody>
            {jobsQuery.isPending ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载…
                </td>
              </tr>
            ) : shown.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {(jobsQuery.data?.jobs ?? []).length === 0
                    ? "还没有执行过任务"
                    : "没有匹配的任务"}
                </td>
              </tr>
            ) : (
              jobVisible.map((job) => {
                const meta = jobStatusMeta(job.status);
                const cmdline = [job.command, ...(job.args ?? [])].join(" ");
                return (
                  <tr
                    key={job.id}
                    onClick={() => navigate(`/jobs/${job.id}`)}
                    title={`点击查看实时输出 · ${cmdline}`}
                    className="cursor-pointer border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="px-4 py-2.5 align-top font-mono text-label-12 text-gray-900">
                      {formatDateTime(job.started_at)}
                    </td>
                    <td className="max-w-md px-4 py-2.5 align-top">
                      <span className="line-clamp-2 font-mono text-label-13 text-gray-1000 [overflow-wrap:anywhere]">
                        {cmdline}
                      </span>
                      {job.output && (
                        <span className="mt-0.5 block truncate font-mono text-label-12 text-gray-900">
                          {job.output.split("\n").find((l) => l.trim()) || ""}
                        </span>
                      )}
                    </td>
                    <td className="px-4 py-2.5 align-top">
                      <span className="rounded border border-gray-400 px-1.5 py-0.5 text-label-12 text-gray-900">
                        {job.task_id ? "定时" : "快速"}
                      </span>
                    </td>
                    <td className={`px-4 py-2.5 align-top text-label-13 ${meta.cls}`}>
                      {meta.label}
                      {job.status === "running" && (
                        <span className="ml-1.5 inline-block h-3 w-3 animate-spin rounded-full border border-blue-1000 border-t-transparent align-[-1px]" />
                      )}
                    </td>
                    <td
                      className={`px-4 py-2.5 text-right align-top font-mono text-label-13 tabular-nums ${
                        job.exit_code == null
                          ? "text-gray-900"
                          : job.exit_code === 0
                            ? "text-green-1000"
                            : "text-red-1000"
                      }`}
                    >
                      {job.exit_code ?? "—"}
                    </td>
                    <td className="px-4 py-2.5 text-right align-top font-mono text-label-13 text-gray-900">
                      {formatDuration(job.started_at, job.finished_at)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            共 {jobs.length} 条{search ? ` · 匹配 ${shown.length} 条` : ""}
          </span>
          <span className="text-label-12">
            {jobs[0]?.started_at ? `最后执行 ${relativeTime(jobs[0].started_at, jobsQuery.data?.at)}` : ""}
          </span>
        </div>
      </div>

      {drawerNonce > 0 && (
        <ScheduleDrawer
          key={drawerNonce}
          agentId={agent?.id}
          onClose={() => setDrawerNonce(0)}
        />
      )}
    </div>
  );
}

/** 定时任务创建抽屉（F30：命令 + 参数 + 间隔预设 chips + 自定义秒数）。 */
function ScheduleDrawer({ agentId, onClose }: { agentId: string | undefined; onClose: () => void }) {
  const PRESETS = [60, 300, 3_600, 21_600, 86_400];
  const [command, setCommand] = useState("");
  const [argsText, setArgsText] = useState("");
  const [interval, setIntervalSecs] = useState(3_600);

  const mutation = useMutation({
    mutationFn: () =>
      api("/api/v1/tasks/schedule", {
        method: "POST",
        body: {
          agent_id: agentId,
          command: command.trim(),
          args: argsText.split("\n").map((l) => l.trim()).filter(Boolean),
          interval_secs: interval,
        },
      }),
    onSuccess: () => {
      toast("已创建，Server 重启后自动恢复");
      onClose();
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const valid = command.trim().length > 0 && !!agentId && interval > 0;

  return (
    <div className="fixed inset-0 z-50">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) mutation.mutate();
        }}
        className="absolute right-0 top-0 flex h-full w-[400px] flex-col gap-5 overflow-y-auto border-l border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-20">定时任务</h2>

        <div>
          <label htmlFor="task-command" className="block text-label-14">
            命令
          </label>
          <input
            id="task-command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="/usr/local/bin/clean.sh"
            autoFocus
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <label htmlFor="task-args" className="block text-label-14">
            参数（每行一个）
          </label>
          <textarea
            id="task-args"
            value={argsText}
            onChange={(e) => setArgsText(e.target.value)}
            rows={3}
            className="mt-2 w-full resize-y rounded-md border border-gray-400 bg-gray-100 px-3 py-2 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <span className="block text-label-14">间隔</span>
          <div className="mt-2 flex flex-wrap gap-2">
            {PRESETS.map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setIntervalSecs(p)}
                className={`h-7 rounded-full border px-3 font-mono text-label-12 transition-colors duration-150 ${
                  interval === p ? "border-gray-1000 bg-gray-200 text-gray-1000" : "border-gray-500 text-gray-900 hover:border-gray-600"
                }`}
              >
                {intervalLabel(p)}
              </button>
            ))}
          </div>
          <div className="mt-3 flex items-center gap-2">
            <input
              type="number"
              min={1}
              value={interval}
              onChange={(e) => setIntervalSecs(Math.max(1, Number(e.target.value) || 0))}
              aria-label="自定义间隔秒数"
              className="h-8 w-28 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <span className="text-label-13 text-gray-900">秒（自定义）</span>
          </div>
        </div>

        {mutation.isError && (
          <p className="text-label-13 text-red-1000" role="alert">
            ⚠ {(mutation.error as Error).message}
          </p>
        )}

        <div className="mt-auto flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={mutation.isPending}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={!valid || mutation.isPending}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {mutation.isPending ? "创建中…" : "创建"}
          </button>
        </div>
      </form>
    </div>
  );
}
