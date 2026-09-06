import { useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import type { components } from "../api/schema";
import { api } from "../api/client";
import { useWsStream } from "../api/ws";
import { toast } from "../lib/toast";
import { kindDot, markReadLocal, parseNotificationFrame, type NotificationItem } from "../lib/notificationStore";
import { relativeTime } from "../lib/format";

type Host = components["schemas"]["Host"];

const KIND_FILTERS = ["all", "online", "offline", "alert"] as const;

/** /notifications 通知中心（规格 notifications.md F59–F61：筛选 + 分页 + 已读）。 */
export default function Notifications() {
  const [params, setParams] = useSearchParams();
  const kind = params.get("kind") ?? "all";
  const unreadOnly = params.get("unread") === "true";
  const page = Math.max(1, Number(params.get("page") ?? 1));
  const limit = 20;
  const queryClient = useQueryClient();
  const [confirmAll, setConfirmAll] = useState(false);

  // 页面常驻 WS：新通知到达即刷新列表（实时性；断线由轮询兜底）
  useWsStream("/api/v1/notifications/stream", (raw) => {
    if (parseNotificationFrame(raw)) queryClient.invalidateQueries({ queryKey: ["notifications"] });
  });

  const listQuery = useQuery({
    queryKey: ["notifications", "page", kind, unreadOnly, page],
    queryFn: async () => {
      const q = new URLSearchParams({ page: String(page), limit: String(limit) });
      if (kind !== "all") q.set("kind", kind);
      if (unreadOnly) q.set("unread", "true");
      const r = await api<{ notifications: NotificationItem[] }>(`/api/v1/notifications?${q}`);
      return { at: Date.now(), notifications: r.notifications ?? [] };
    },
    refetchInterval: 30_000,
  });
  const hostsQuery = useQuery({
    queryKey: ["hosts", "all"],
    queryFn: () => api<{ hosts: Host[] }>("/api/v1/hosts?page=1&limit=200"),
  });
  const host = (id: string) => (hostsQuery.data?.hosts ?? []).find((h) => h.id === id);

  const markOne = useMutation({
    mutationFn: (id: string) => api(`/api/v1/notifications/${id}/read`, { method: "POST" }),
    onSuccess: () => {
      markReadLocal(1); // 铃铛角标即时减一
      queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const readAll = useMutation({
    mutationFn: () => api<{ updated: number }>("/api/v1/notifications/read-all", { method: "POST" }),
    onSuccess: (r) => {
      setConfirmAll(false);
      markReadLocal(r.updated);
      toast(`已标记 ${r.updated} 条`);
      queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  function patchParams(patch: Record<string, string | null>) {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(patch)) {
      if (!v || v === "all" || v === "false") next.delete(k);
      else next.set(k, v);
    }
    if (!("page" in patch)) next.delete("page");
    setParams(next, { replace: true });
  }

  const items = listQuery.data?.notifications ?? [];
  const unreadTotal = items.filter((n) => !n.read).length;

  return (
    <div className="flex flex-col gap-6">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="text-copy-13 text-gray-900">主机上下线与预警的系统内通知</p>
        </div>
        <button
          type="button"
          onClick={() => setConfirmAll(true)}
          disabled={unreadTotal === 0}
          className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
        >
          全部标为已读
        </button>
      </div>

      <div className="flex flex-wrap items-center gap-2">
        {KIND_FILTERS.map((k) => {
          const dot = k === "all" ? null : kindDot(k);
          return (
            <button
              key={k}
              type="button"
              onClick={() => patchParams({ kind: k })}
              aria-pressed={kind === k}
              className={`h-7 rounded-full border px-3 text-label-13 transition-colors duration-150 ${
                kind === k
                  ? "border-gray-1000 bg-gray-200 text-gray-1000"
                  : "border-gray-500 text-gray-900 hover:border-gray-600"
              }`}
            >
              {dot && <span className={`mr-1 ${dot.cls}`}>{dot.symbol}</span>}
              {k === "all" ? "全部" : dot!.label}
            </button>
          );
        })}
        <span className="flex-1" />
        <label className="flex cursor-pointer items-center gap-2 text-label-13 text-gray-900">
          <input
            type="checkbox"
            checked={unreadOnly}
            onChange={(e) => patchParams({ unread: String(e.target.checked) })}
            className="h-3.5 w-3.5 accent-[#0070f3]"
          />
          只看未读
        </label>
      </div>

      <div className="overflow-visible rounded-lg border border-gray-400">
        <table className="w-full text-left">
          <thead>
            <tr className="border-b border-gray-400 text-label-13 text-gray-900">
              <th className="w-10 px-4 py-2 font-normal">状态</th>
              <th className="px-4 py-2 font-normal">通知内容</th>
              <th className="px-4 py-2 font-normal">主机</th>
              <th className="px-4 py-2 font-normal">时间</th>
              <th className="px-4 py-2" />
            </tr>
          </thead>
          <tbody>
            {listQuery.isPending ? (
              <tr>
                <td colSpan={5} className="px-4 py-10 text-center text-label-13 text-gray-900">
                  加载通知…
                </td>
              </tr>
            ) : items.length === 0 ? (
              <tr>
                <td colSpan={5} className="px-4 py-12 text-center text-label-13 text-gray-900">
                  {unreadOnly ? "没有未读通知 ✓" : "暂无通知——主机上下线与预警会出现在这里"}
                </td>
              </tr>
            ) : (
              items.map((n) => {
                const dot = kindDot(n.kind);
                const h = host(n.host_id);
                return (
                  <tr
                    key={n.id}
                    className={`group border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
                      n.read ? "text-gray-900" : ""
                    }`}
                  >
                    <td className="px-4 py-2.5">
                      <span className={`text-label-13 ${dot.cls}`} title={dot.label}>
                        {dot.symbol}
                      </span>
                    </td>
                    <td className="px-4 py-2.5 text-label-14">
                      {n.message}
                      {!n.read && <span className="ml-2 inline-block h-1.5 w-1.5 rounded-full bg-blue-1000 align-middle" />}
                    </td>
                    <td className="px-4 py-2.5 text-label-13">
                      {h ? (
                        <Link to={`/hosts/${h.id}/overview`} className="text-blue-1000 hover:underline">
                          {h.hostname}
                        </Link>
                      ) : (
                        "—"
                      )}
                    </td>
                    <td className="px-4 py-2.5 font-mono text-label-13 text-gray-900">
                      {relativeTime(n.created_at, listQuery.data?.at)}
                    </td>
                    <td className="px-4 py-2.5 text-right">
                      {!n.read && (
                        <button
                          type="button"
                          onClick={() => markOne.mutate(n.id)}
                          className="h-7 rounded-md border border-gray-500 px-2.5 text-label-12 opacity-0 transition-opacity duration-150 group-hover:opacity-100 hover:bg-gray-200"
                        >
                          标为已读
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
        <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
          <span>{items.length > 0 ? `第 ${page} 页` : ""}</span>
          <span className="flex items-center gap-3">
            <button
              type="button"
              disabled={page <= 1}
              onClick={() => patchParams({ page: String(page - 1) })}
              className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
            >
              ‹ 上一页
            </button>
            <button
              type="button"
              disabled={items.length < limit}
              onClick={() => patchParams({ page: String(page + 1) })}
              className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
            >
              下一页 ›
            </button>
          </span>
        </div>
      </div>

      {/* 全部已读确认模态（规格 F60） */}
      {confirmAll && (
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <button
            type="button"
            aria-label="关闭"
            onClick={() => setConfirmAll(false)}
            className="absolute inset-0 bg-black/40"
          />
          <div className="relative z-10 w-[380px] rounded-xl border border-gray-400 bg-background-100 p-6">
            <h2 className="text-heading-16">全部标为已读</h2>
            <p className="mt-3 text-label-13 text-gray-900">将所有未读通知标为已读？</p>
            <div className="mt-6 flex justify-end gap-3">
              <button
                type="button"
                onClick={() => setConfirmAll(false)}
                disabled={readAll.isPending}
                className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
              >
                取消
              </button>
              <button
                type="button"
                onClick={() => readAll.mutate()}
                disabled={readAll.isPending}
                className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
              >
                {readAll.isPending ? "标记中…" : "确认"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
