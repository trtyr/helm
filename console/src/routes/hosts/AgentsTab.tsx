import { useQuery } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { relativeTime } from "../../lib/format";

type Agent = components["schemas"]["Agent"];

/** Agents 视图（规格 routes/hosts.md）：agent_id mono / 版本 / 所属主机 / 最后心跳；
 * 在线态由最后心跳 30s 内推导（列表 API 无 online 字段，M1 降级方案）。 */
export function AgentsTab() {
  const { data, isPending, isError, error, refetch } = useQuery({
    queryKey: ["agents"],
    queryFn: async () => {
      const res = await api<{ agents: Agent[] }>("/api/v1/agents");
      // fetchedAt 在异步侧产生，render 内不再调用 Date.now（纯度）
      return { ...res, fetchedAt: Date.now() };
    },
    refetchInterval: 30_000,
  });
  const now = data?.fetchedAt ?? 0;

  if (isPending) {
    return (
      <div className="animate-pulse space-y-2 p-4">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="h-9 rounded bg-gray-200" />
        ))}
      </div>
    );
  }
  if (isError) {
    return (
      <div className="p-8 text-center text-label-13 text-red-1000">
        加载失败：{error.message}
        <button onClick={() => refetch()} className="ml-3 text-blue-1000 hover:underline">
          重试
        </button>
      </div>
    );
  }

  const agents = data.agents ?? [];
  if (agents.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-gray-500 py-16">
        <p className="text-copy-13 text-gray-900">
          暂无注册 Agent——在目标机安装 helm-agent 后出现
        </p>
      </div>
    );
  }

  const online = (a: Agent) => {
    if (!a.last_heartbeat_at) return false;
    return now - new Date(a.last_heartbeat_at).getTime() < 30_000;
  };

  return (
    <table className="w-full text-left">
      <thead>
        <tr className="border-b border-gray-400 text-label-13 text-gray-900">
          <th className="px-4 py-2 font-normal">状态</th>
          <th className="px-4 py-2 font-normal">Agent ID</th>
          <th className="px-4 py-2 font-normal">版本</th>
          <th className="px-4 py-2 font-normal">所属主机</th>
          <th className="px-4 py-2 font-normal">最后心跳</th>
        </tr>
      </thead>
      <tbody>
        {agents.map((a) => (
          <tr
            key={a.id}
            className="border-b border-gray-400/60 transition-colors duration-150 hover:bg-gray-100"
          >
            <td className="px-4 py-2.5">
              <span
                title={online(a) ? "在线（心跳 30s 内）" : "离线"}
                className={`inline-block h-2 w-2 rounded-full ${
                  online(a) ? "bg-green-1000" : "border border-gray-900"
                }`}
              />
            </td>
            <td className="px-4 py-2.5 font-mono text-label-13">{a.id}</td>
            <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900">
              {a.version ?? "—"}
            </td>
            <td className="px-4 py-2.5 text-label-14">{a.hostname ?? "—"}</td>
            <td className="px-4 py-2.5 text-label-13 text-gray-900">
              {relativeTime(a.last_heartbeat_at, now)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
