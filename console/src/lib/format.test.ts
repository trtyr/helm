import { describe, expect, it } from "vitest";
import { relativeTime } from "./format";

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
