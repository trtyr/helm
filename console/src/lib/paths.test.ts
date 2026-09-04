import { describe, expect, it } from "vitest";
import { crumbLabel, crumbSegments, humanSize, joinPath, parentPath } from "./paths";

describe("joinPath", () => {
  it("根目录拼接不加双斜杠", () => {
    expect(joinPath("/", "tmp")).toBe("/tmp");
    expect(joinPath("", "tmp")).toBe("/tmp");
  });
  it("常规拼接", () => {
    expect(joinPath("/var/log", "nginx")).toBe("/var/log/nginx");
  });
});

describe("parentPath", () => {
  it("逐级回退，根返回 null", () => {
    expect(parentPath("/var/log/nginx")).toBe("/var/log");
    expect(parentPath("/var")).toBe("/");
    expect(parentPath("/")).toBeNull();
  });
});

describe("crumbSegments / crumbLabel", () => {
  it("路径 → 面包屑段序列", () => {
    expect(crumbSegments("/var/log/nginx")).toEqual(["/", "/var", "/var/log", "/var/log/nginx"]);
    expect(crumbSegments("/")).toEqual(["/"]);
  });
  it("段显示名", () => {
    expect(crumbLabel("/")).toBe("/");
    expect(crumbLabel("/var/log")).toBe("log");
  });
});

describe("humanSize", () => {
  it("阶梯与进位", () => {
    expect(humanSize(0)).toBe("0 B");
    expect(humanSize(512)).toBe("512 B");
    expect(humanSize(1536)).toBe("1.5 KB");
    expect(humanSize(15 * 1024 * 1024)).toBe("15.0 MB");
  });
});
