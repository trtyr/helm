import { describe, expect, it } from "vitest";
import { ifaceKind } from "./net";

describe("ifaceKind 接口类型推断", () => {
  it("回环", () => {
    expect(ifaceKind("lo")).toBe("回环接口");
    expect(ifaceKind("lo0")).toBe("回环接口");
  });
  it("常规网络", () => {
    expect(ifaceKind("eth0")).toBe("常规网络接口");
    expect(ifaceKind("enp0s3")).toBe("常规网络接口");
    expect(ifaceKind("wlan0")).toBe("常规网络接口");
  });
  it("容器 / 网桥", () => {
    expect(ifaceKind("docker0")).toBe("容器 / 网桥");
    expect(ifaceKind("vetha1b2c3")).toBe("容器 / 网桥");
    expect(ifaceKind("br-123abc")).toBe("容器 / 网桥");
    expect(ifaceKind("bridge100")).toBe("容器 / 网桥");
  });
  it("隧道与无线接力（macOS）", () => {
    expect(ifaceKind("utun4")).toBe("隧道接口");
    expect(ifaceKind("awdl0")).toBe("本地无线接力");
    expect(ifaceKind("llw10")).toBe("本地无线接力");
  });
  it("未识别返回 null（不标注）", () => {
    expect(ifaceKind("gif0")).toBeNull();
    expect(ifaceKind("stf0")).toBeNull();
  });
});
