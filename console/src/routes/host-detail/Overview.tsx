import { useState } from "react";
import { useOutletContext, Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { FolderOpen, SquareTerminal } from "lucide-react";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { relativeTime } from "../../lib/format";

type HostView = components["schemas"]["HostView"];
type Metric = components["schemas"]["Metric"];
type Agent = components["schemas"]["Agent"];
type Job = components["schemas"]["Job"];

const OS_LABEL: Record<string, string> = {
  windows: "Windows",
  linux: "Linux",
  macos: "macOS",
  darwin: "macOS",
};

interface Ctx {
  host: HostView;
}

/** /hosts/:id/overview：系统负载快照 + Agent + 信息 + 最近任务。命令执行走「终端」页签。 */
export default function Overview() {
  const { host } = useOutletContext<Ctx>();

  const metricsQuery = useQuery({
    queryKey: ["metrics", host.id],
    queryFn: () =>
      api<{ metrics: Metric[] }>(`/api/v1/metrics?host_id=${host.id}&limit=50`),
    refetchInterval: 30_000,
  });
  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const jobsQuery = useQuery({
    queryKey: ["jobs"],
    queryFn: () => api<{ jobs: Job[] }>("/api/v1/jobs?page=1&limit=100"),
    refetchInterval: 30_000,
  });

  const agentsForHost = (agentsQuery.data?.agents ?? []).filter((a) => a.host_id === host.id);

  // 负载快照：每指标最新值（后端按时间倒序，取每 name 首个）
  const snapshot = new Map<string, number>();
  for (const m of metricsQuery.data?.metrics ?? []) {
    if (m.name && !snapshot.has(m.name)) snapshot.set(m.name, m.value ?? 0);
  }

  const recentJobs = (jobsQuery.data?.jobs ?? [])
    .filter((j) => j.host_id === host.id)
    .slice(0, 5);

  const [evidenceBusy, setEvidenceBusy] = useState(false);
  const agent = (agentsQuery.data?.agents ?? []).find((a) => a.host_id === host.id);

  const downloadEvidence = async () => {
    if (!agentsQuery.data) return;
    setEvidenceBusy(true);
    try {
      const r = await fetch("/api/v1/ir/evidence", {
        method: "POST",
        headers: { "Content-Type": "application/json", Authorization: `Bearer ${localStorage.getItem("helm-console.token")}` },
        body: JSON.stringify({ agent_id: agent?.id }),
      });
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const blob = await r.blob();
      const a = document.createElement("a");
      a.href = URL.createObjectURL(blob);
      a.download = (r.headers.get("content-disposition") ?? "").match(/filename="([^"]+)"/)?.[1] ?? "evidence.json";
      a.click();
      URL.revokeObjectURL(a.href);
    } catch (e) {
      alert(`证据包导出失败: ${(e as Error).message}`);
    } finally {
      setEvidenceBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-6">
      {/* 快捷入口 */}
      <div className="flex flex-wrap gap-3">
        <Link
          to={`/hosts/${host.id}/terminal`}
          className="flex h-9 items-center gap-2 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800"
        >
          <SquareTerminal size={15} strokeWidth={1.5} />
          打开终端
        </Link>

        <button
          type="button"
          disabled={!host.online || evidenceBusy}
          onClick={downloadEvidence}
          className="flex h-9 items-center gap-2 rounded-md border border-gray-500 px-4 text-label-14 text-gray-900 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
        >
          {evidenceBusy ? "收集证据中…" : "导出证据包"}
        </button>
        <Link
          to={`/hosts/${host.id}/files`}
          className="flex h-9 items-center gap-2 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
        >
          <FolderOpen size={15} strokeWidth={1.5} />
          文件管理
        </Link>
      </div>

      <div className="grid grid-cols-12 gap-6">
        {/* 左列 */}
        <div className="col-span-12 flex flex-col gap-6 lg:col-span-7">
          {/* 负载快照 */}
          <div className="rounded-lg border border-gray-400 p-6">
            <h2 className="text-heading-16">系统负载</h2>
            <div className="mt-4 flex flex-col gap-3">
              {(
                [
                  ["CPU", "cpu.usage"],
                  ["内存", "mem.percent"],
                  ["磁盘", "disk.usage"],
                ] as const
              ).map(([label, key]) => {
                const v = snapshot.get(key);
                const over = v !== undefined && v > 90;
                return (
                  <div key={key} className="flex items-center gap-3">
                    <span className="w-10 text-label-13 text-gray-900">{label}</span>
                    <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-gray-200">
                      {v !== undefined && (
                        <div
                          className={`h-full rounded-full ${over ? "bg-amber-1000" : "bg-blue-1000"}`}
                          style={{ width: `${Math.min(100, v)}%` }}
                        />
                      )}
                    </div>
                    <span
                      className={`w-16 text-right font-mono text-label-13 tabular-nums ${
                        over ? "text-amber-1000" : "text-gray-1000"
                      }`}
                    >
                      {v !== undefined ? `${v.toFixed(1)}%` : "—"}
                    </span>
                  </div>
                );
              })}
              {metricsQuery.isPending && (
                <p className="animate-pulse text-label-13 text-gray-900">加载指标…</p>
              )}
              {!metricsQuery.isPending && snapshot.size === 0 && (
                <p className="text-label-13 text-gray-900">
                  暂无上报数据——Agent 每 30s 上报一批
                </p>
              )}
            </div>
          </div>

          {/* 最近任务 */}
          <div className="rounded-lg border border-gray-400 p-6">
            <div className="flex items-center justify-between">
              <h2 className="text-heading-16">最近任务</h2>
              <Link
                to={`/hosts/${host.id}/tasks`}
                className="text-label-13 text-blue-1000 transition-colors duration-150 hover:underline"
              >
                全部任务 →
              </Link>
            </div>
            <div className="mt-3 flex flex-col">
              {recentJobs.map((j) => (
                <div
                  key={j.id}
                  className="flex items-center gap-3 border-b border-gray-400/60 py-2 text-label-13 last:border-0"
                >
                  <span className="font-mono text-gray-900">#{j.id?.slice(0, 4)}</span>
                  <span className="min-w-0 flex-1 truncate font-mono">{j.command}</span>
                  <StatusText status={j.status} />
                  <span className="text-gray-900">{relativeTime(j.finished_at ?? j.started_at)}</span>
                </div>
              ))}
              {recentJobs.length === 0 && (
                <p className="py-4 text-label-13 text-gray-900">还没有执行过任务</p>
              )}
            </div>
          </div>
        </div>

        {/* 右列 */}
        <div className="col-span-12 flex flex-col gap-6 lg:col-span-5">
          {/* Agent */}
          <div className="rounded-lg border border-gray-400 p-6">
            <h2 className="text-heading-16">Agent</h2>
            <div className="mt-3 flex flex-col gap-3">
              {agentsForHost.map((a) => (
                <div key={a.id} className="flex items-center gap-3 text-label-13">
                  <span
                    className={`inline-block h-2 w-2 rounded-full ${
                      heartbeatRecent(a.last_heartbeat_at) ? "bg-green-1000" : "border border-gray-900"
                    }`}
                  />
                  <span className="font-mono">{a.id}</span>
                  <span className="text-gray-900">v{a.version ?? "?"}</span>
                  <span className="ml-auto text-gray-900">
                    心跳 {relativeTime(a.last_heartbeat_at)}
                  </span>
                </div>
              ))}
              {agentsForHost.length === 0 && (
                <p className="py-2 text-label-13 text-gray-900">未注册——主机上线后自动出现</p>
              )}
            </div>
          </div>

          {/* 主机信息 */}
          <div className="rounded-lg border border-gray-400 p-6">
            <h2 className="text-heading-16">主机信息</h2>
            <dl className="mt-3 flex flex-col">
              {(
                [
                  ["系统", host.os ? OS_LABEL[host.os] ?? host.os : "—"],
                  ["系统版本", host.os_version || "—"],
                  ["内核", host.kernel || "—"],
                  ["开机", host.boot_at ? relativeTime(host.boot_at) : "—"],
                  ["架构", host.arch ?? "—"],
                  ["平台", host.platform ?? "—"],
                  ["模式", host.conn_mode === "forward" ? "正向" : "反向"],
                  ["地址", host.addr || (host.conn_mode === "forward" ? "（未配置）" : "—")],
                  ["host_id", host.id ?? "—"],
                ] as const
              ).map(([k, v]) => (
                <div
                  key={k}
                  className="flex items-center justify-between border-b border-gray-400/60 py-2 last:border-0"
                >
                  <dt className="text-label-13 text-gray-900">{k}</dt>
                  <dd className="font-mono text-label-13">{v}</dd>
                </div>
              ))}
            </dl>
          </div>
        </div>
      </div>
    </div>
  );
}

function StatusText({ status }: { status?: string }) {
  const map: Record<string, [string, string]> = {
    succeeded: ["✓ 成功", "text-green-1000"],
    failed: ["✗ 失败", "text-red-1000"],
    timed_out: ["⏱ 超时", "text-amber-1000"],
    running: ["● 运行中", "text-blue-1000"],
    queued: ["○ 排队", "text-gray-900"],
    cancelled: ["— 取消", "text-gray-900"],
  };
  const [label, cls] = map[status ?? ""] ?? [status ?? "—", "text-gray-900"];
  return <span className={`${cls} font-medium`}>{label}</span>;
}

function heartbeatRecent(iso: string | null | undefined): boolean {
  if (!iso) return false;
  return Date.now() - new Date(iso).getTime() < 30_000;
}
