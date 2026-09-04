import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bell } from "lucide-react";
import { api } from "../api/client";
import { useWsStream } from "../api/ws";
import { toast } from "../lib/toast";
import {
  badgeText,
  feedSnapshot,
  kindDot,
  markReadLocal,
  parseNotificationFrame,
  pushNotification,
  setUnreadCount,
  subscribeFeed,
  type NotificationItem,
} from "../lib/notificationStore";
import { relativeTime } from "../lib/format";

/**
 * Topbar 铃铛（F57/F58）：角标（WS 即时 + 30s 兜底轮询）+ 360px 下拉
 * （最近 10 条 / 未读 2px 指示条 / 全部已读 / 查看全部）。AppLayout 挂载一次。
 */
export function NotificationBell() {
  const [open, setOpen] = useState(false);
  const feed = useSyncExternalStore(subscribeFeed, feedSnapshot);
  const badge = badgeText(feed.unread);
  const queryClient = useQueryClient();
  const navigate = useNavigate();

  // WS 实时（binary 帧已在 useWsStream 解码为文本）+ 兜底轮询
  useWsStream(
    "/api/v1/notifications/stream",
    (raw) => {
      const n = parseNotificationFrame(raw);
      if (n) pushNotification(n);
    },
    { enabled: true },
  );
  const unreadQuery = useQuery({
    queryKey: ["notifications", "unread-count"],
    queryFn: async () => {
      const r = await api<{ count: number }>("/api/v1/notifications/unread-count");
      setUnreadCount(r.count);
      return r;
    },
    refetchInterval: 30_000,
  });
  void unreadQuery;

  // 下拉打开时拉最近 10 条
  const listQuery = useQuery({
    queryKey: ["notifications", "recent"],
    queryFn: async () => {
      const r = await api<{ notifications: NotificationItem[] }>("/api/v1/notifications?page=1&limit=10");
      return { at: Date.now(), notifications: r.notifications ?? [] }; // now 异步侧（render 纯度）
    },
    enabled: open,
  });

  const readAll = useMutation({
    mutationFn: () => api<{ updated: number }>("/api/v1/notifications/read-all", { method: "POST" }),
    onSuccess: (r) => {
      markReadLocal(feed.unread);
      toast(`已标记 ${r.updated} 条`);
      queryClient.invalidateQueries({ queryKey: ["notifications"] });
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const markOne = useMutation({
    mutationFn: (id: string) => api(`/api/v1/notifications/${id}/read`, { method: "POST" }),
  });

  // Esc 关闭 + 点外关闭
  const boxRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    const onClick = (e: MouseEvent) => {
      if (!boxRef.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("keydown", onKey);
    document.addEventListener("mousedown", onClick);
    return () => {
      document.removeEventListener("keydown", onKey);
      document.removeEventListener("mousedown", onClick);
    };
  }, [open]);

  function openItem(n: NotificationItem) {
    setOpen(false);
    if (!n.read) {
      markOne.mutate(n.id);
      markReadLocal(1);
    }
    // online/offline → 主机概览；alert → 主机指标（规格 notifications.md）
    if (n.kind === "alert") {
      navigate(`/hosts/${n.host_id}/metrics`);
    } else {
      navigate(`/hosts/${n.host_id}/overview`);
    }
  }

  return (
    <div ref={boxRef} className="relative">
      <button
        type="button"
        aria-label={`通知（${feed.unread} 条未读）`}
        onClick={() => setOpen((v) => !v)}
        className="relative flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
      >
        <Bell
          size={16}
          strokeWidth={1.5}
          className={feed.pulse > 0 ? "animate-[pulse-once_200ms_ease-out]" : undefined}
          key={feed.pulse}
        />
        {badge && (
          <span className="absolute -right-0.5 -top-0.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-blue-1000 px-1 font-mono text-[10px] leading-none text-white">
            {badge}
          </span>
        )}
      </button>

      {open && (
        <div className="absolute right-0 top-10 z-40 w-[360px] overflow-hidden rounded-xl border border-gray-400 bg-background-100 shadow-lg">
          <div className="flex items-center justify-between border-b border-gray-400 px-4 py-2.5">
            <span className="text-label-13">
              通知 {feed.unread > 0 && <span className="text-blue-1000">●{feed.unread}</span>}
            </span>
            <button
              type="button"
              onClick={() => readAll.mutate()}
              disabled={readAll.isPending || feed.unread === 0}
              className="text-label-12 text-gray-900 transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
            >
              全部标为已读
            </button>
          </div>
          <div className="max-h-[480px] overflow-y-auto">
            {listQuery.isPending ? (
              <div className="space-y-2 p-4">
                {Array.from({ length: 5 }).map((_, i) => (
                  <div key={i} className="h-5 animate-pulse rounded bg-gray-200" />
                ))}
              </div>
            ) : (listQuery.data?.notifications ?? []).length === 0 ? (
              <p className="p-6 text-center text-label-13 text-gray-900">暂无通知</p>
            ) : (
              (listQuery.data?.notifications ?? []).map((n) => {
                const dot = kindDot(n.kind);
                return (
                  <button
                    key={n.id}
                    type="button"
                    onClick={() => openItem(n)}
                    className={`flex w-full items-center gap-2.5 border-b border-gray-400/50 px-4 py-2.5 text-left transition-colors duration-150 last:border-0 hover:bg-gray-200/60 ${
                      n.read ? "opacity-55" : ""
                    }`}
                  >
                    {!n.read && <span className="h-8 w-0.5 shrink-0 rounded bg-blue-1000" />}
                    <span className={`shrink-0 text-label-13 ${dot.cls}`}>{dot.symbol}</span>
                    <span className="min-w-0 flex-1 truncate text-label-13">{n.message}</span>
                    <span className="shrink-0 font-mono text-label-12 text-gray-900">
                      {relativeTime(n.created_at, listQuery.data?.at)}
                    </span>
                  </button>
                );
              })
            )}
          </div>
          <div className="border-t border-gray-400 px-4 py-2 text-right">
            <Link
              to="/notifications"
              onClick={() => setOpen(false)}
              className="text-label-12 text-blue-1000 hover:underline"
            >
              查看全部 →
            </Link>
          </div>
        </div>
      )}
    </div>
  );
}
