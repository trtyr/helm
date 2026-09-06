import { describe, expect, it } from "vitest";
import { crumbLabel, crumbSegments, humanSize, joinPath, parentPath } from "./paths";

describe("joinPath", () => {
  it("POSIX 根目录拼接不加双斜杠", () => {
    expect(joinPath("/", "tmp")).toBe("/tmp");
  });
  it("此电脑根（空串）拼接 Windows 驱动器", () => {
    expect(joinPath("", "C:\\")).toBe("C:\\");
  });
  it("Windows 常规拼接", () => {
    expect(joinPath("C:\\", "Users")).toBe("C:\\Users");
    expect(joinPath("C:\\Users", "pub")).toBe("C:\\Users\\pub");
  });
  it("POSIX 常规拼接", () => {
    expect(joinPath("/var/log", "nginx")).toBe("/var/log/nginx");
  });
});

describe("parentPath", () => {
  it("POSIX 逐级回退，根返回 null", () => {
    expect(parentPath("/var/log/nginx")).toBe("/var/log");
    expect(parentPath("/var")).toBe("/");
    expect(parentPath("/")).toBeNull();
  });
  it("Windows 驱动器根回退到此电脑", () => {
    expect(parentPath("C:\\Users\\pub")).toBe("C:\\Users");
    expect(parentPath("C:\\Users")).toBe("C:\\");
    expect(parentPath("C:\\")).toBe("");
    expect(parentPath("")).toBeNull();
  });
});

describe("crumbSegments / crumbLabel", () => {
  it("POSIX 路径 → 面包屑段序列", () => {
    expect(crumbSegments("/var/log/nginx")).toEqual(["/", "/var", "/var/log", "/var/log/nginx"]);
    expect(crumbSegments("/")).toEqual(["/"]);
  });
  it("Windows 路径 → 面包屑段序列（此电脑打头）", () => {
    expect(crumbSegments("C:\\Users\\pub")).toEqual(["", "C:\\", "C:\\Users", "C:\\Users\\pub"]);
    expect(crumbSegments("C:\\")).toEqual(["", "C:\\"]);
  });
  it("段显示名", () => {
    expect(crumbLabel("/")).toBe("/");
    expect(crumbLabel("/var/log")).toBe("log");
    expect(crumbLabel("")).toBe("此电脑");
    expect(crumbLabel("C:\\")).toBe("C:");
    expect(crumbLabel("C:\\Users")).toBe("Users");
  });
});

describe("humanSize", () => {
  it("字节人类可读", () => {
    expect(humanSize(1)).toBe("1 B");
    expect(humanSize(1536)).toBe("1.5 KB");
    expect(humanSize(1024 * 1024)).toBe("1.0 MB");
  });
});
