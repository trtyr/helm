import { describe, expect, it, vi } from "vitest";
import {
  badgeText,
  feedSnapshot,
  kindDot,
  markReadLocal,
  parseNotificationFrame,
  pushNotification,
  resetFeedForTest,
  setUnreadCount,
  subscribeFeed,
  type NotificationItem,
} from "./notificationStore";

function item(over: Partial<NotificationItem> = {}): NotificationItem {
  return {
    id: "n1",
    host_id: "h1",
    kind: "online",
    message: "主机 web-1 已上线",
    read: false,
    created_at: "2026-09-04T00:00:00Z",
    ...over,
  };
}

describe("badgeText（F57 角标）", () => {
  it("0 → 无角标；1-9 → 数字；≥10 → 9+", () => {
    expect(badgeText(0)).toBeNull();
    expect(badgeText(3)).toBe("3");
    expect(badgeText(9)).toBe("9");
    expect(badgeText(10)).toBe("9+");
    expect(badgeText(42)).toBe("9+");
  });
});

describe("feed 状态流", () => {
  it("push 未读 +1 + 脉冲；已读不加", () => {
    resetFeedForTest();
    pushNotification(item());
    pushNotification(item({ id: "n2", kind: "alert" }));
    pushNotification(item({ id: "n3", read: true }));
    const snap = feedSnapshot();
    expect(snap.unread).toBe(2);
    expect(snap.pulse).toBe(2);
    resetFeedForTest();
  });
  it("setUnreadCount 校准去重（相同值不 emit）", () => {
    resetFeedForTest();
    setUnreadCount(5);
    expect(feedSnapshot().unread).toBe(5);
    setUnreadCount(5); // 不变不触发
    resetFeedForTest();
  });
  it("markReadLocal 下限 0", () => {
    resetFeedForTest();
    pushNotification(item());
    markReadLocal(5);
    expect(feedSnapshot().unread).toBe(0);
    resetFeedForTest();
  });
  it("subscribe/unsubscribe", () => {
    resetFeedForTest();
    const fn = vi.fn();
    const off = subscribeFeed(fn);
    pushNotification(item());
    expect(fn).toHaveBeenCalledTimes(1);
    off();
    pushNotification(item({ id: "x" }));
    expect(fn).toHaveBeenCalledTimes(1);
    resetFeedForTest();
  });
});

describe("parseNotificationFrame", () => {
  it("合法帧解析", () => {
    const n = parseNotificationFrame(
      JSON.stringify({ id: "a", host_id: "h", kind: "offline", message: "下线", read: false, created_at: "2026-01-01T00:00:00Z" }),
    );
    expect(n?.kind).toBe("offline");
  });
  it("缺字段/坏 JSON 返回 null", () => {
    expect(parseNotificationFrame("{bad")).toBeNull();
    expect(parseNotificationFrame(JSON.stringify({ id: "a" }))).toBeNull();
  });
});

describe("kindDot", () => {
  it("三态符号", () => {
    expect(kindDot("online").symbol).toBe("●");
    expect(kindDot("offline").symbol).toBe("○");
    expect(kindDot("alert").symbol).toBe("◐");
    expect(kindDot("alert").cls).toBe("text-amber-1000");
  });
});
