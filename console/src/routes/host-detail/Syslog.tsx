import { useOutletContext } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type IrFinding = components["schemas"]["IrFinding"];

interface Ctx {
  host: HostView;
}

/** 系统日志审计：ir_scan types=["events"]。 */
export default function Syslog() {
  const { host } = useOutletContext<Ctx>();
  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const scanQuery = useQuery({
    queryKey: ["ir-events", agent?.id],
    queryFn: () =>
      api<{ findings: IrFinding[]; error?: string | null }>("/api/v1/ir/scan", {
        method: "POST",
        body: { agent_id: agent?.id, types: ["events"] },
      }),
    enabled: !!agent && host.online,
  });

  const findings = scanQuery.data?.findings ?? [];
  const sorted = [...findings].sort((a, b) => {
    const order = (s: string) => (s === "critical" ? 0 : s === "warn" ? 1 : 2);
    return order(a.severity ?? "info") - order(b.severity ?? "info");
  });

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-label-13 text-gray-900">系统日志</span>
        <span className="font-mono text-label-12 text-gray-900">{findings.length} 条</span>
        <button
          type="button"
          onClick={() => scanQuery.refetch()}
          disabled={!host.online}
          className="ml-auto h-8 rounded-md border border-gray-500 px-3 text-label-13 hover:bg-gray-200 disabled:opacity-40"
        >
          {scanQuery.isFetching ? "扫描中…" : "重新扫描"}
        </button>
      </div>
      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-16 px-3 py-2.5 font-normal">级别</th>
              <th className="w-44 whitespace-nowrap px-3 py-2.5 font-normal">名称</th>
              <th className="w-full max-w-0 px-4 py-2.5 font-normal">详情</th>
            </tr>
          </thead>
          <tbody>
            {scanQuery.isPending ? (
              <tr><td colSpan={3} className="px-4 py-10 text-center text-label-13 text-gray-900">扫描中…</td></tr>
            ) : scanQuery.isError ? (
              <tr><td colSpan={3} className="px-4 py-10 text-center">
                <span className="text-label-13 text-red-1000">{(scanQuery.error as Error).message}</span>
                <button onClick={() => scanQuery.refetch()} className="ml-3 text-label-13 text-blue-1000 hover:underline">重试</button>
              </td></tr>
            ) : findings.length === 0 ? (
              <tr><td colSpan={3} className="px-4 py-10 text-center text-label-13 text-gray-900">无发现</td></tr>
            ) : (
              sorted.map((f, i) => (
                <tr key={i} className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100">
                  <td className="px-3 py-2">
                    <span className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${f.severity === "critical" ? "bg-red-1000/10 text-red-1000" : f.severity === "warn" ? "bg-amber-1000/10 text-amber-1000" : "bg-gray-200 text-gray-900"}`}>
                      {f.severity === "critical" ? "严重" : f.severity === "warn" ? "可疑" : "信息"}
                    </span>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 text-label-13">{f.name}</td>
                  <td className="max-w-0 px-4 py-2">
                    <span className="block truncate font-mono text-label-12 text-gray-900" title={f.detail}>{f.detail}</span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          共 {findings.length} 条
        </div>
      </div>
    </div>
  );
}
