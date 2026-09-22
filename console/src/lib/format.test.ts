import { describe, expect, it } from "vitest";
import { formatUptime, relativeTime } from "./format";

const T0 = new Date("2026-09-03T12:00:00Z").getTime();

describe("relativeTime", () => {
  it("空值显示占位符", () => {
    expect(relativeTime(null)).toBe("—");
    expect(relativeTime(undefined)).toBe("—");
  });

  it("秒/分/时/天 阶梯", () => {
    expect(relativeTime(new Date(T0 - 5_000).toISOString(), T0)).toBe("5s 前");
    expect(relativeTime(new Date(T0 - 3 * 60_000).toISOString(), T0)).toBe("3m 前");
    expect(relativeTime(new Date(T0 - 2 * 3_600_000).toISOString(), T0)).toBe("2h 前");
    expect(relativeTime(new Date(T0 - 3 * 86_400_000).toISOString(), T0)).toBe("3d 前");
  });

  it("未来时间（时钟偏差）按刚刚处理", () => {
    expect(relativeTime(new Date(T0 + 10_000).toISOString(), T0)).toBe("刚刚");
  });
});

describe("formatUptime（P006 P0-9：now 参数单位是毫秒）", () => {
  // 进程启动时刻用「unix 秒」，而第二参数（now）是 **Date.now() 毫秒**。
  // 旧代码在调用处传了 `Date.now() / 1000`，导致内部再除一次 1000 → 差值恒为负 → 恒显示 0s。
  const started = Math.floor(T0 / 1000); // unix 秒

  it("毫秒 now → 正确阶梯", () => {
    expect(formatUptime(started, T0)).toBe("0s");
    expect(formatUptime(started - 30, T0)).toBe("30s");
    expect(formatUptime(started - 5 * 60, T0)).toBe("5m");
    expect(formatUptime(started - 2 * 3_600, T0)).toBe("2h 0m");
    expect(formatUptime(started - 3 * 86_400, T0)).toBe("3d 0h");
  });

  it("旧调用点的错法（now 传秒）→ 被钳到 0s：这就是那个 bug 的形状", () => {
    // 正确用法：now = 毫秒
    expect(formatUptime(started - 3_600, T0)).toBe("1h 0m");
    // 旧代码把 `Date.now() / 1000` 传进 now（秒当毫秒）→ 内部再除一次 1000 → 差值恒为负
    // → Math.max(0, …) 钳成 0 → 表格里永远显示 0s
    expect(formatUptime(started - 3_600, T0 / 1000)).toBe("0s");
  });

  it("空值占位", () => {
    expect(formatUptime(undefined, T0)).toBe("—");
    expect(formatUptime(0, T0)).toBe("—");
  });
});
