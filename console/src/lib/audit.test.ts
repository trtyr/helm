import { describe, expect, it } from "vitest";
import { auditResource } from "./audit";

describe("auditResource", () => {
  it("null/undefined → —", () => {
    expect(auditResource(null)).toBe("—");
    expect(auditResource(undefined)).toBe("—");
  });
  it("按优先级取标识字段", () => {
    expect(auditResource({ hostname: "web-1", agent_id: "a1" })).toBe("web-1");
    expect(auditResource({ job_id: "abc", path: "/x" })).toBe("abc");
    expect(auditResource({ path: "/etc/nginx.conf" })).toBe("/etc/nginx.conf");
  });
  it("长值截断 24 + 省略号", () => {
    expect(auditResource({ command: "x".repeat(40) })).toBe(`${"x".repeat(24)}…`);
  });
  it("无标识字段回退首个字符串值", () => {
    expect(auditResource({ foo: "bar" })).toBe("bar");
  });
  it("全非字符串 → —", () => {
    expect(auditResource({ n: 1, b: true })).toBe("—");
  });
});
