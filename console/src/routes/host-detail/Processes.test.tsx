import { describe, expect, it } from "vitest";
import { buildTreeRows, suspiciousPair } from "./Processes";
import type { components } from "../../api/schema";

type ProcessInfo = components["schemas"]["ProcessInfo"];

function proc(pid: number, name: string, parent = 0): ProcessInfo {
  return { pid, name, parent_pid: parent, cpu_percent: 0, mem_bytes: 0 } as ProcessInfo;
}

describe("suspiciousPair 异常父子规则", () => {
  it("办公软件派生解释器", () => {
    expect(suspiciousPair("WINWORD.EXE", "powershell.exe")).not.toBeNull();
    expect(suspiciousPair("excel.exe", "cmd.exe")).not.toBeNull();
  });
  it("浏览器派生解释器", () => {
    expect(suspiciousPair("chrome.exe", "mshta.exe")).not.toBeNull();
    expect(suspiciousPair("msedge.exe", "rundll32.exe")).not.toBeNull();
  });
  it("LSASS 派生任意进程 = 高危", () => {
    expect(suspiciousPair("lsass.exe", "whatever.exe")).toContain("LSASS");
  });
  it("SMSS 只允许三类子进程", () => {
    expect(suspiciousPair("smss.exe", "csrss.exe")).toBeNull();
    expect(suspiciousPair("smss.exe", "notepad.exe")).toContain("SMSS");
  });
  it("services.exe 派生解释器", () => {
    expect(suspiciousPair("services.exe", "cmd.exe")).toContain("服务管理器");
    expect(suspiciousPair("services.exe", "svchost.exe")).toBeNull();
  });
  it("正常关系不误报", () => {
    expect(suspiciousPair("explorer.exe", "notepad.exe")).toBeNull();
    expect(suspiciousPair("bash.exe", "bash.exe")).toBeNull();
    expect(suspiciousPair("services.exe", "svchost.exe")).toBeNull();
    expect(suspiciousPair("", "cmd.exe")).toBeNull();
  });
});

describe("buildTreeRows 进程树构建", () => {
  it("标准 Windows 树：System→smss→csrss 链 + 孤儿归根", () => {
    const procs = [
      proc(0, "[System Process]"),
      proc(4, "System", 0),
      proc(300, "smss.exe", 4),
      proc(400, "csrss.exe", 300),
      proc(500, "wininit.exe", 300),
      proc(900, "chrome.exe", 12345), // 父进程不在快照 → 归根
    ];
    const rows = buildTreeRows(procs, new Set());
    const depthOf = (name: string) => rows.find((r) => r.p.name === name)?.depth ?? -1;
    expect(depthOf("[System Process]")).toBe(0);
    expect(depthOf("System")).toBe(1);
    expect(depthOf("smss.exe")).toBe(2);
    expect(depthOf("csrss.exe")).toBe(3);
    expect(depthOf("chrome.exe")).toBe(0); // 孤儿 = 根
    expect(rows).toHaveLength(6);
  });

  it("折叠：折叠父进程后其子树不出现在行序中", () => {
    const procs = [
      proc(0, "[System Process]"),
      proc(4, "System", 0),
      proc(300, "smss.exe", 4),
      proc(400, "csrss.exe", 300),
    ];
    const rows = buildTreeRows(procs, new Set([300]));
    expect(rows.some((r) => r.p.name === "csrss.exe")).toBe(false);
    expect(rows.some((r) => r.p.name === "smss.exe")).toBe(true);
  });

  it("环引用不死循环", () => {
    const procs = [proc(1, "a.exe", 2), proc(2, "b.exe", 1)];
    const rows = buildTreeRows(procs, new Set());
    expect(rows.length).toBeGreaterThan(0);
  });

  it("兄弟按名称排序", () => {
    const procs = [
      proc(0, "[System Process]"),
      proc(4, "System", 0),
      proc(10, "zzz.exe", 4),
      proc(11, "aaa.exe", 4),
    ];
    const rows = buildTreeRows(procs, new Set());
    const names = rows.filter((r) => r.depth === 2).map((r) => r.p.name);
    expect(names).toEqual(["aaa.exe", "zzz.exe"]);
  });
});
