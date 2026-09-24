import { useQuery } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";

import { PaginationBar } from "../../components/pagination";
import { FilterBar } from "../../components/filterBar";
import { api } from "../../api/client";

/** 状态事件行（对应 server status_events 表，P003 T1；ended_at 为 LEAD 窗口，P003 T6）。 */
interface StatusEvent {
  id: string;
  host_id: string;
  event: "online" | "offline";
  reason: string;
  detail: string;
  created_at: string;
  ended_at?: string | null;
}

interface HostRow {
  id: string;
  hostname: string;
}

const REASON_LABEL: Record<string, string> = {
  registered: "注册上线",
  transport_error: "传输错误",
  stream_closed: "连接关闭",
  stopped: "服务停止",
};

function fmtTime(s: string): string {
  const d = new Date(s);
  return d.toLocaleString("zh-CN", { hour12: false });
}

/** 离线时长：ended_at（LEAD）有值显示已恢复时长，否则显示进行中。 */
function fmtDuration(created: string, ended?: string | null): string {
  if (!ended) return "进行中";
  const mins = Math.floor(
    (new Date(ended).getTime() - new Date(created).getTime()) / 60_000,
  );
  if (mins < 1) return "<1 分钟";
  return `${mins} 分钟`;
}

/** URL 参数补丁：改任何筛选都回到第 1 页（null/空串删除键，同 Jobs 页 patchParams 语义）。 */
function patchParams(
  params: URLSearchParams,
  patch: Record<string, string | null>,
): URLSearchParams {
  const next = new URLSearchParams(params);
  for (const [k, v] of Object.entries(patch)) {
    if (v === null || v === "") next.delete(k);
    else next.set(k, v);
  }
  if (!("page" in patch)) next.delete("page");
  return next;
}

/**
 * 状态事件子页（P003 T6）：agent 上下线与断连原因的时间线。
 * 服务端筛选（q/event/host_id）+ 服务端排序（created_at/event）+ PaginationBar；
 * 筛选/排序/分页全部走 URL 参数（可收藏可分享）。
 */
export default function Events() {
  const [params, setParams] = useSearchParams();
  const page = Number(params.get("page") ?? "1") || 1;
  const limit = Number(params.get("limit") ?? "50") || 50;
  const sort = params.get("sort") ?? "";
  const q = params.get("q") ?? "";
  const event = params.get("event") ?? "";
  const hostId = params.get("host_id") ?? "";
  const range = params.get("range") ?? "";
  const from = params.get("from") ?? "";
  const to = params.get("to") ?? "";

  const onPatch = (patch: Record<string, string | null>) =>
    setParams(patchParams(params, patch), { replace: true });

  // 主机名映射（events 只带 host_id uuid，展示层翻译成 hostname）
  const hostsQ = useQuery({
    queryKey: ["hosts-for-events"],
    queryFn: () => api<{ hosts: HostRow[] }>("/api/v1/hosts?limit=200"),
    staleTime: 60_000,
  });
  const hostName = (id: string) =>
    hostsQ.data?.hosts.find((h) => h.id === id)?.hostname ?? id;

  const listQ = useQuery({
    queryKey: ["status-events", page, limit, sort, q, event, hostId, from, to],
    queryFn: () => {
      const sp = new URLSearchParams({ page: String(page), limit: String(limit) });
      if (sort) sp.set("sort", sort);
      if (q) sp.set("q", q);
      if (event) sp.set("event", event);
      if (hostId) sp.set("host_id", hostId);
      if (from) sp.set("from", from);
      if (to) sp.set("to", to);
      return api<{ events: StatusEvent[]; total: number }>(
        `/api/v1/logs/events?${sp.toString()}`,
      );
    },
    refetchInterval: 30_000,
  });
  const events = listQ.data?.events ?? [];
  const total = listQ.data?.total ?? 0;

  // 服务端排序（field / field:desc / field:asc），列头三态循环
  const sortField = sort.replace(":desc", "").replace(":asc", "");
  const sortDesc = sort.endsWith(":desc");
  const sortIcon = (key: string) =>
    sortField === key ? (sortDesc ? " ↓" : " ↑") : "";
  const toggleSort = (key: string) => {
    const cur = sortField === key ? (sortDesc ? "desc" : "asc") : "";
    const next = cur === "" ? "asc" : cur === "asc" ? "desc" : "";
    onPatch({ sort: next ? `${key}:${next}` : null });
  };

  return (
    <div className="flex flex-col gap-4">
      <div className="flex items-baseline justify-between">
        <h2 className="text-heading-16">状态事件</h2>
        <span className="text-label-12 text-gray-900">共 {total} 条</span>
      </div>

      {/* 筛选栏（P003 T7：FilterBar 时间/主机统一组件 + 本页关键字/类型） */}
      <div className="flex flex-wrap items-center gap-2">
        <input
          value={q}
          onChange={(e) => onPatch({ q: e.target.value || null })}
          placeholder="搜索原因/详情/主机…"
          className="h-7 w-56 rounded-md border border-gray-400 px-2 text-label-13"
        />
        <select
          value={event}
          onChange={(e) => onPatch({ event: e.target.value || null })}
          aria-label="按类型筛选"
          className="h-7 rounded-md border border-gray-400 px-2 text-label-13"
        >
          <option value="">类型：全部</option>
          <option value="online">上线</option>
          <option value="offline">离线</option>
        </select>
        <FilterBar value={{ range, from, to, host_id: hostId }} onPatch={onPatch} />
      </div>

      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-left">
          <thead>
            <tr className="border-b border-gray-400">
              <th
                className="cursor-pointer px-4 py-2 text-label-13 whitespace-nowrap select-none"
                onClick={() => toggleSort("created_at")}
              >
                时间{sortIcon("created_at")}
              </th>
              <th className="px-4 py-2 text-label-13 whitespace-nowrap">主机</th>
              <th
                className="cursor-pointer px-4 py-2 text-label-13 whitespace-nowrap select-none"
                onClick={() => toggleSort("event")}
              >
                类型{sortIcon("event")}
              </th>
              <th className="px-4 py-2 text-label-13 whitespace-nowrap">原因</th>
              <th className="px-4 py-2 text-label-13 whitespace-nowrap">离线时长</th>
              <th className="px-4 py-2 text-label-13 whitespace-nowrap">详情</th>
            </tr>
          </thead>
          <tbody>
            {events.map((e) => (
              <tr key={e.id} className="border-b border-gray-400/60">
                <td className="px-4 py-2 text-label-13 font-mono whitespace-nowrap">
                  {fmtTime(e.created_at)}
                </td>
                <td className="px-4 py-2 text-label-13 whitespace-nowrap">
                  {hostName(e.host_id)}
                </td>
                <td className="px-4 py-2 text-label-13 whitespace-nowrap">
                  <span
                    className={`rounded px-1.5 py-0.5 ${
                      e.event === "online"
                        ? "bg-green-1000/10 text-green-1000"
                        : "bg-gray-300 text-gray-1000"
                    }`}
                  >
                    {e.event === "online" ? "上线" : "离线"}
                  </span>
                </td>
                <td className="px-4 py-2 text-label-13 whitespace-nowrap">
                  {REASON_LABEL[e.reason] ?? e.reason}
                </td>
                <td className="px-4 py-2 text-label-13 whitespace-nowrap">
                  {e.event === "offline" ? fmtDuration(e.created_at, e.ended_at) : "—"}
                </td>
                <td className="px-4 py-2 text-label-12 text-gray-900 max-w-[240px] truncate">
                  {e.detail || "—"}
                </td>
              </tr>
            ))}
            {events.length === 0 && (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-center text-label-13 text-gray-900">
                  {listQ.isLoading ? "加载中…" : "暂无状态事件"}
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      <PaginationBar
        page={page}
        limit={limit}
        total={total}
        onPatch={(patch) => onPatch(patch)}
      />
    </div>
  );
}
