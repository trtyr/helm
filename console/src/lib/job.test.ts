import { describe, expect, it } from "vitest";
import { intervalLabel, jobKindLabel, jobStatusMeta } from "./job";

describe("jobStatusMeta", () => {
  it("四态 + 未知容错", () => {
    expect(jobStatusMeta("succeeded").label).toBe("✓ 成功");
    expect(jobStatusMeta("failed").cls).toBe("text-red-1000");
    expect(jobStatusMeta("running").cls).toBe("text-blue-1000");
    expect(jobStatusMeta("queued").label).toBe("○ 排队中");
    expect(jobStatusMeta(undefined).label).toBe("—");
    expect(jobStatusMeta("weird").label).toBe("weird");
  });
});

describe("jobKindLabel", () => {
  it("task_id 有无推导", () => {
    expect(jobKindLabel("abc")).toBe("定时");
    expect(jobKindLabel(null)).toBe("快速");
    expect(jobKindLabel(undefined)).toBe("快速");
  });
});

describe("intervalLabel", () => {
  it("阶梯取整", () => {
    expect(intervalLabel(86_400)).toBe("每 1d");
    expect(intervalLabel(3_600)).toBe("每 1h");
    expect(intervalLabel(21_600)).toBe("每 6h");
    expect(intervalLabel(300)).toBe("每 5m");
    expect(intervalLabel(60)).toBe("每 1m");
    expect(intervalLabel(45)).toBe("每 45s");
  });
});
