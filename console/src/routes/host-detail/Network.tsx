import { useMemo, useState } from "react";
import { useOutletContext, useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { RefreshCw, Search, X } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { ifaceKind } from "../../lib/net";
import { toast } from "../../lib/toast";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];
type NetInterface = components["schemas"]["NetInterface"];
type NetConnection = components["schemas"]["NetConnection"];

interface Ctx {
  host: HostView;
}

type ProtoFilter = "all" | "tcp" | "udp";
type Tab = "interfaces" | "connections";

/** 网络（双标签）：「网络接口」= 适配器卡片；「连接」= TCP/UDP 会话表。
 * 数据同源（一次 net info 采集），标签只切视图。 */
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
    await navigator.clipboard.writeText(text);
    toast(`已复制 ${text}`);
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

function TabBtn({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`relative -mb-px flex h-10 items-center text-label-14 transition-colors duration-150 ${
        active ? "text-gray-1000" : "text-gray-900 hover:text-gray-1000"
      }`}
    >
      {children}
      {active && (
        <span className="absolute inset-x-0 bottom-0 h-0.5 rounded-full bg-blue-1000" />
      )}
    </button>
  );
}

// ---------------------------------------------------------------------------
// 标签一：网络接口
// ---------------------------------------------------------------------------

function InterfacesView({
  netQuery,
  onCopy,
}: {
  netQuery: { data?: { interfaces: NetInterface[]; hostname: string; at: number } };
  onCopy: (text: string) => void;
}) {
  const interfaces = netQuery.data?.interfaces ?? [];
  return (
    <div className="flex flex-col gap-4">
      {netQuery.data === undefined ? (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {Array.from({ length: 3 }).map((_, i) => (
            <div key={i} className="h-28 animate-pulse rounded-lg border border-gray-400 bg-gray-100" />
          ))}
        </div>
      ) : interfaces.length === 0 ? (
        <div className="rounded-lg border border-dashed border-gray-500 p-8 text-center text-label-13 text-gray-900">
          暂无网络接口信息
        </div>
      ) : (
        <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
          {interfaces.map((iface, idx) => {
            const up = (iface.status ?? "") === "up";
            return (
              <div
                key={`${iface.name}-${idx}`}
                className={`rounded-lg border bg-background-100 p-4 ${
                  up ? "border-gray-400" : "border-gray-400/50 opacity-70"
                }`}
              >
                <div className="flex items-baseline gap-2">
                  <h3 className="font-mono text-label-16">{iface.name}</h3>
                  {(iface.kind || ifaceKind(iface.name ?? "")) && (
                    <span className="text-label-12 text-gray-900">
                      {iface.kind || ifaceKind(iface.name ?? "")}
                    </span>
                  )}
                  <span
                    className={`ml-auto flex items-center gap-1 text-label-12 ${
                      up ? "text-green-1000" : "text-gray-900"
                    }`}
                  >
                    <span
                      className={`h-1.5 w-1.5 rounded-full ${
                        up ? "bg-green-1000" : "border border-gray-600"
                      }`}
                    />
                    {iface.status || "unknown"}
                  </span>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  {(iface.addrs ?? []).map((addr) => (
                    <button
                      key={addr}
                      type="button"
                      onClick={() => onCopy(addr)}
                      title="点击复制"
                      className="flex items-center gap-1.5 rounded border border-gray-400 bg-gray-100 px-2 py-1 font-mono text-label-12 transition-colors duration-150 hover:border-gray-500 hover:bg-gray-200"
                    >
                      {addr}
                      {addr.includes(":") && (
                        <span className="rounded-sm border border-gray-500 px-0.5 text-[10px] leading-tight text-gray-900">
                          v6
                        </span>
                      )}
                    </button>
                  ))}
                  {(iface.addrs ?? []).length === 0 && (
                    <span className="text-label-12 text-gray-900">无地址</span>
                  )}
                </div>
                {(iface.mac || iface.gateway) && (
                  <div className="mt-2.5 flex flex-wrap gap-x-4 font-mono text-label-12 text-gray-900">
                    {iface.mac && (
                      <span title="物理地址">
                        MAC {iface.mac}
                        <button
                          type="button"
                          aria-label="复制 MAC"
                          onClick={() => onCopy(iface.mac ?? "")}
                          className="ml-1 text-blue-1000 opacity-60 hover:opacity-100"
                        >
                          ⧉
                        </button>
                      </span>
                    )}
                    {iface.gateway && <span>网关 {iface.gateway}</span>}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
      {netQuery.data?.hostname && (
        <p>
          主机名{" "}
          <button
            type="button"
            onClick={() => onCopy(netQuery.data!.hostname)}
            title="点击复制"
            className="font-mono text-label-14 text-blue-1000 hover:underline"
          >
            {netQuery.data.hostname}
          </button>
        </p>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// 标签二：连接
// ---------------------------------------------------------------------------

function ConnectionsView({
  netQuery,
  proto,
  setProto,
  search,
  setSearch,
}: {
  netQuery: { data?: { connections: NetConnection[]; at: number }; isFetching: boolean };
  proto: ProtoFilter;
  setProto: (p: ProtoFilter) => void;
  search: string;
  setSearch: (s: string) => void;
}) {
  const connections = useMemo(() => {
    let list = netQuery.data?.connections ?? [];
    if (proto !== "all") list = list.filter((c) => c.protocol === proto);
    const q = search.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (c) =>
          (c.local ?? "").toLowerCase().includes(q) ||
          (c.remote ?? "").toLowerCase().includes(q) ||
          (c.process_name ?? "").toLowerCase().includes(q) ||
          (c.state ?? "").toLowerCase().includes(q) ||
          String(c.pid ?? "") === q,
      );
    }
    return list;
  }, [netQuery.data, proto, search]);

  const stateCounts = useMemo(() => {
    const all = netQuery.data?.connections ?? [];
    return {
      total: all.length,
      established: all.filter((c) => c.state?.toUpperCase() === "ESTABLISHED").length,
      listening: all.filter((c) => c.state?.toUpperCase() === "LISTENING").length,
      timeWait: all.filter((c) => c.state?.toUpperCase() === "TIME_WAIT").length,
    };
  }, [netQuery.data]);

  async function copy(text: string) {
    await navigator.clipboard.writeText(text);
    toast(`已复制 ${text}`);
  }

  return (
    <div className="flex flex-col gap-4">
      {/* 工具行：协议过滤 + 状态计数 + 搜索 */}
      <div className="flex flex-wrap items-center gap-2">
        {(["all", "tcp", "udp"] as const).map((p) => (
          <button
            key={p}
            type="button"
            onClick={() => setProto(p)}
            aria-pressed={proto === p}
            className={`h-7 rounded-full border px-3 font-mono text-label-12 transition-colors duration-150 ${
              proto === p
                ? "border-gray-1000 bg-gray-200 text-gray-1000"
                : "border-gray-500 text-gray-900 hover:border-gray-600"
            }`}
          >
            {p.toUpperCase()}
          </button>
        ))}
        <span className="ml-2 font-mono text-label-12 text-gray-900">
          ESTABLISHED {stateCounts.established} · LISTENING {stateCounts.listening} · TIME_WAIT{" "}
          {stateCounts.timeWait}
        </span>
        <div className="relative ml-auto">
          <Search size={13} strokeWidth={1.5} className="absolute left-2.5 top-1/2 -translate-y-1/2 text-gray-900" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="搜索地址 / 端口 / 进程 / 状态"
            aria-label="搜索连接"
            className="h-8 w-60 rounded-md border border-gray-400 bg-gray-100 pl-8 pr-7 text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
          {search && (
            <button
              type="button"
              aria-label="清除搜索"
              onClick={() => setSearch("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-900 hover:text-gray-1000"
            >
              <X size={12} strokeWidth={1.5} />
            </button>
          )}
        </div>
      </div>

      <div className="overflow-hidden rounded-lg border border-gray-400 bg-background-100">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-16 whitespace-nowrap px-3 py-2.5 font-normal">协议</th>
              <th className="w-1/3 px-4 py-2.5 font-normal">本地地址</th>
              <th className="w-full max-w-0 px-4 py-2.5 font-normal">远程地址</th>
              <th className="w-32 whitespace-nowrap px-3 py-2.5 font-normal">状态</th>
              <th className="w-16 whitespace-nowrap px-3 py-2.5 text-right font-normal">PID</th>
              <th className="w-44 max-w-44 whitespace-nowrap px-3 py-2.5 font-normal">进程</th>
            </tr>
          </thead>
          <tbody>
            {netQuery.data === undefined ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载连接表…
                </td>
              </tr>
            ) : connections.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  {search || proto !== "all" ? "没有匹配的连接" : "没有活动连接"}
                </td>
              </tr>
            ) : (
              connections.slice(0, 300).map((c, i) => {
                const state = (c.state ?? "").toUpperCase();
                return (
                  <tr
                    key={`${c.protocol}-${c.local}-${c.remote}-${i}`}
                    className="border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100"
                  >
                    <td className="whitespace-nowrap px-3 py-2 font-mono text-label-12 text-gray-900 uppercase">
                      {c.protocol}
                    </td>
                    <td className="max-w-0 truncate px-4 py-2 font-mono text-label-13 text-gray-1000">
                      <button
                        type="button"
                        onClick={() => copy(c.local ?? "")}
                        title={`点击复制 · ${c.local}`}
                        className="block w-full truncate text-left hover:text-blue-1000"
                      >
                        {c.local}
                      </button>
                    </td>
                    <td className="max-w-0 truncate px-4 py-2 font-mono text-label-13 text-gray-900" title={c.remote}>
                      {c.remote}
                    </td>
                    <td className="whitespace-nowrap px-3 py-2">
                      <span
                        className={`rounded px-1.5 py-0.5 font-mono text-label-12 ${
                          state === "ESTABLISHED"
                            ? "bg-green-1000/10 text-green-1000"
                            : state === "LISTENING"
                              ? "bg-blue-1000/10 text-blue-1000"
                              : state === "TIME_WAIT" || state === "CLOSE_WAIT"
                                ? "bg-amber-1000/10 text-amber-1000"
                                : "text-gray-900"
                        }`}
                      >
                        {c.state || "—"}
                      </span>
                    </td>
                    <td className="whitespace-nowrap px-3 py-2 text-right font-mono text-label-13 text-gray-900 tabular-nums">
                      {c.pid ? c.pid : "—"}
                    </td>
                    <td className="max-w-44 truncate whitespace-nowrap px-3 py-2 font-mono text-label-13" title={c.process_name}>
                      {c.process_name || "—"}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>
            共 {stateCounts.total} 条连接{search || proto !== "all" ? ` · 匹配 ${connections.length}` : ""}
            {connections.length > 300 ? " · 显示前 300" : ""}
          </span>
          <span className="text-label-12">
            {netQuery.data ? `快照 ${new Date(netQuery.data.at).toLocaleTimeString()}` : ""}
          </span>
        </div>
      </div>
      <p className="text-label-12 text-gray-900">
        连接表来自目标机系统 TCP/UDP 路由表（Windows 原生 API 实时采集）；PID 为 0 表示无权限查看归属进程。
      </p>
    </div>
  );
}
