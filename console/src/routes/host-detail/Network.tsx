import { useState } from "react";
import { copyText } from "../../lib/clipboard";
import { useOutletContext, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { toast } from "../../lib/toast";
import { ConnectionsView, InterfacesView, TabBtn, type ProtoFilter } from "./network/NetworkViews";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type NetInterface = components["schemas"]["NetInterface"];
type NetConnection = components["schemas"]["NetConnection"];

interface Ctx {
  host: HostView;
}

type Tab = "interfaces" | "connections";

/**
 * 网络（双标签）：「网络接口」= 适配器卡片；「连接」= TCP/UDP 会话表。
 * 数据同源（一次 net info 采集），标签只切视图。
 *
 * G13 拆分（2026-09-21）：原为 485 行单文件。现拆为——`network/NetworkViews.tsx`
 * （标签按钮、接口视图、连接视图）；本文件保留标签路由、数据获取与工具行。
 */
export default function Network() {
  const { host } = useOutletContext<Ctx>();
  const [params, setParams] = useSearchParams();
  const tab: Tab = params.get("tab") === "connections" ? "connections" : "interfaces";
  const [proto, setProto] = useState<ProtoFilter>("all");
  const [search, setSearch] = useState("");
  const [auto, setAuto] = useState(true);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  const netQuery = useQuery({
    queryKey: ["net-info", agent?.id],
    queryFn: async () => {
      const r = await api<{
        hostname?: string;
        interfaces?: NetInterface[];
        connections?: NetConnection[];
      }>("/api/v1/net/info", {
        method: "POST",
        body: { agent_id: agent!.id },
      });
      return {
        at: Date.now(),
        hostname: r.hostname ?? "",
        interfaces: r.interfaces ?? [],
        connections: r.connections ?? [],
      };
    },
    enabled: !!agent && host.online,
    refetchInterval: auto ? 10_000 : false,
  });

  async function copy(text: string) {
    const ok = await copyText(text);
    ok ? toast(`已复制 ${text}`) : toast("复制失败", "warn");
  }

  function setTab(next: Tab) {
    const next_ = new URLSearchParams(params);
    if (next === "interfaces") next_.delete("tab");
    else next_.set("tab", next);
    setParams(next_, { replace: true });
  }

  const offline = !host.online;

  return (
    <div className="flex flex-col gap-4">
      {/* 标签栏 + 刷新控件 */}
      <div className="flex items-center gap-6 border-b border-gray-400">
        <TabBtn active={tab === "interfaces"} onClick={() => setTab("interfaces")}>
          网络接口
          <span className="ml-1.5 font-mono text-label-12 text-gray-900">
            {netQuery.data?.interfaces.length ?? "…"}
          </span>
        </TabBtn>
        <TabBtn active={tab === "connections"} onClick={() => setTab("connections")}>
          连接
          <span className="ml-1.5 font-mono text-label-12 text-gray-900">
            {netQuery.data?.connections.length ?? "…"}
          </span>
        </TabBtn>
        <span className="ml-auto flex items-center gap-2 pb-2">
          <button
            type="button"
            aria-label={auto ? "关闭自动刷新" : "开启自动刷新"}
            title={auto ? "自动刷新：开" : "自动刷新：关"}
            onClick={() => setAuto((a) => !a)}
            className={`flex h-8 items-center gap-1.5 rounded-md border px-2 font-mono text-label-12 transition-colors duration-150 ${
              auto ? "border-blue-1000 text-blue-1000" : "border-gray-400 text-gray-900"
            }`}
          >
            ⏱ 10s
          </button>
          <button
            type="button"
            aria-label="刷新"
            onClick={() => netQuery.refetch()}
            disabled={offline || netQuery.isFetching}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
          >
            <RefreshCw size={14} strokeWidth={1.5} className={netQuery.isFetching ? "animate-spin" : ""} />
          </button>
        </span>
      </div>

      {netQuery.isError ? (
        <div className="rounded-lg border border-gray-400 p-8 text-center">
          <p className="text-label-13 text-red-1000">网络信息获取失败：{(netQuery.error as Error).message}</p>
          <button
            type="button"
            onClick={() => netQuery.refetch()}
            className="mt-3 h-8 rounded-md border border-gray-500 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-200"
          >
            重试
          </button>
        </div>
      ) : tab === "interfaces" ? (
        <InterfacesView netQuery={netQuery} onCopy={copy} />
      ) : (
        <ConnectionsView
          netQuery={netQuery}
          proto={proto}
          setProto={setProto}
          search={search}
          setSearch={setSearch}
        />
      )}
    </div>
  );
}
