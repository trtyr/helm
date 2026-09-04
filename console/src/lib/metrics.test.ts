import { describe, expect, it } from "vitest";
import { bucketByName, bytesLabel, limitForRange, pushWindow } from "./metrics";

describe("limitForRange", () => {
  it("按 30s 密度推导", () => {
    expect(limitForRange("live")).toBe(60);
    expect(limitForRange("1h")).toBe(120);
    expect(limitForRange("6h")).toBe(144);
    expect(limitForRange("24h")).toBe(288);
  });
});

describe("pushWindow", () => {
  it("追加 + 超窗 shift 头", () => {
    let w = [{ ts: 1, value: 10 }, { ts: 2, value: 20 }];
    w = pushWindow(w, { ts: 3, value: 30 }, 3);
    expect(w).toHaveLength(3);
    w = pushWindow(w, { ts: 4, value: 40 }, 3);
    expect(w).toHaveLength(3);
    expect(w[0].ts).toBe(2);
    expect(w[2].value).toBe(40);
  });
  it("乱序/重复 ts 幂等丢弃", () => {
    const w = [{ ts: 5, value: 1 }];
    expect(pushWindow(w, { ts: 5, value: 2 }, 10)).toHaveLength(1);
    expect(pushWindow(w, { ts: 4, value: 3 }, 10)).toHaveLength(1);
  });
});

describe("bucketByName", () => {
  it("混合列表按 name 分桶 + 同 ts 去重保留最新 + 时间升序", () => {
    const buckets = bucketByName([
      { name: "cpu.usage", value: 10, ts: "2026-09-04T00:00:30Z" },
      { name: "mem.percent", value: 50, ts: "2026-09-04T00:00:30Z" },
      { name: "cpu.usage", value: 12, ts: "2026-09-04T00:00:00Z" },
      { name: "cpu.usage", value: 11, ts: "2026-09-04T00:00:30Z" }, // 覆盖第一条
    ]);
    expect(buckets.get("cpu.usage")).toEqual([
      { ts: new Date("2026-09-04T00:00:00Z").getTime(), value: 12 },
      { ts: new Date("2026-09-04T00:00:30Z").getTime(), value: 11 },
    ]);
    expect(buckets.get("mem.percent")).toHaveLength(1);
  });
  it("缺字段条目跳过", () => {
    const buckets = bucketByName([{ name: "x" }, { value: 1 }, {}]);
    expect(buckets.size).toBe(0);
  });
});

describe("bytesLabel", () => {
  it("阶梯", () => {
    expect(bytesLabel(512)).toBe("512");
    expect(bytesLabel(2048)).toBe("2.0K");
    expect(bytesLabel(5 * 1024 * 1024)).toBe("5.0M");
    expect(bytesLabel(2 * 1024 ** 3)).toBe("2.0G");
  });
});
