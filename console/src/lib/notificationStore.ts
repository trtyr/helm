/** 通知流全局状态（F57 角标跨组件共享：Topbar 铃铛 + toast + 中心页）。 */
import type { ToastKind } from "./toast";
import { toast } from "./toast";

export type NotificationKind = "online" | "offline" | "alert";

export interface NotificationItem {
  id: string;
  host_id: string;
  kind: NotificationKind;
  message: string;
  read: boolean;
  created_at: string;
}

interface FeedState {
  unread: number;
  pulse: number; // 铃铛脉冲信号（递增计数）
  listeners: Set<() => void>;
  toastWindow: { count: number; timer: ReturnType<typeof setTimeout> | null };
  cached: { unread: number; pulse: number };
}

let state: FeedState = {
  unread: 0,
  pulse: 0,
  listeners: new Set(),
  toastWindow: { count: 0, timer: null },
  cached: { unread: 0, pulse: 0 },
};

function refreshCached() {
  state.cached = { unread: state.unread, pulse: state.pulse };
}

export function subscribeFeed(fn: () => void): () => void {
  state.listeners.add(fn);
  return () => state.listeners.delete(fn);
}

function emit() {
  state.listeners.forEach((fn) => fn());
}

/** 兜底轮询校准（30s unread-count）。 */
export function setUnreadCount(count: number) {
  if (state.unread !== count) {
    state.unread = count;
    refreshCached();
    emit();
  }
}

/** WS 新通知到达：角标 +1 + 脉冲 + toast 5s 窗口合并。 */
export function pushNotification(item: NotificationItem) {
  if (!item.read) {
    state.unread += 1;
    state.pulse += 1;
    refreshCached();
    emit();
    queueToast(item);
  }
}

/** 本地标记已读（单条/全部）后的即时反馈。 */
export function markReadLocal(delta: number) {
  state.unread = Math.max(0, state.unread - delta);
  refreshCached();
  emit();
}

/** 缓存快照（useSyncExternalStore 要求引用稳定——每次 getSnapshot 返回同一对象）。 */
export function feedSnapshot(): { unread: number; pulse: number } {
  return state.cached;
}

/** toast 合并：5s 窗口内多条合并为「N 条新通知」。 */
function queueToast(item: NotificationItem) {
  const w = state.toastWindow;
  w.count += 1;
  if (w.timer) return; // 窗口未关：等 flush
  w.timer = setTimeout(() => {
    const count = w.count;
    w.count = 0;
    w.timer = null;
    if (count === 1) {
      toast(firstLine(item.message), kindToToast(item.kind));
    } else {
      toast(`${count} 条新通知`, "ok");
    }
  }, 5_000);
}

function firstLine(message: string): string {
  return message.split("\n")[0] ?? message;
}

function kindToToast(kind: NotificationKind): ToastKind {
  if (kind === "alert") return "warn";
  if (kind === "offline") return "ok";
  return "ok";
}

/** kind → 状态点符号与色（规格 notifications.md：●/◌/◐）。 */
export function kindDot(kind: NotificationKind): { symbol: string; cls: string; label: string } {
  switch (kind) {
    case "online":
      return { symbol: "●", cls: "text-green-1000", label: "上线" };
    case "offline":
      return { symbol: "○", cls: "text-gray-900", label: "下线" };
    default:
      return { symbol: "◐", cls: "text-amber-1000", label: "预警" };
  }
}

/** WS 流 JSON 帧解析（宽容：缺字段帧返回 null）。 */
export function parseNotificationFrame(raw: string): NotificationItem | null {
  try {
    const m = JSON.parse(raw) as Partial<NotificationItem>;
    if (!m.id || !m.kind || !m.message || m.read === undefined) return null;
    return {
      id: m.id,
      host_id: m.host_id ?? "",
      kind: m.kind,
      message: m.message,
      read: m.read,
      created_at: m.created_at ?? new Date(0).toISOString(),
    };
  } catch {
    return null;
  }
}

/** 角标显示文本（≥10 显示 9+，规格 F57）。 */
export function badgeText(unread: number): string | null {
  if (unread <= 0) return null;
  return unread >= 10 ? "9+" : String(unread);
}

/** 测试隔离：重置模块状态。 */
export function resetFeedForTest() {
  if (state.toastWindow.timer) clearTimeout(state.toastWindow.timer);
  state = {
    unread: 0,
    pulse: 0,
    listeners: new Set(),
    toastWindow: { count: 0, timer: null },
    cached: { unread: 0, pulse: 0 },
  };
}
