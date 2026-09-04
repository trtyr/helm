import { describe, expect, it } from "vitest";
import {
  FONT_SIZES,
  DEFAULT_FONT,
  loadTermSettings,
  resolveTermTheme,
  saveTermSettings,
  type TermSettings,
} from "./termSettings";

function memStorage(): Pick<Storage, "getItem" | "setItem"> & { map: Map<string, string> } {
  const map = new Map<string, string>();
  return {
    map,
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, v),
  };
}

describe("loadTermSettings", () => {
  it("空存储回默认（字号 14 / 跟随系统）", () => {
    const s = loadTermSettings(memStorage());
    expect(s.fontSize).toBe(DEFAULT_FONT);
    expect(s.theme).toBe("system");
  });
  it("合法值往返", () => {
    const st = memStorage();
    saveTermSettings({ fontSize: 18, theme: "light" }, st);
    expect(loadTermSettings(st)).toEqual({ fontSize: 18, theme: "light" });
  });
  it("非法值回默认", () => {
    const st = memStorage();
    st.map.set("helm-console.term-font", "99");
    st.map.set("helm-console.term-theme", "solarized");
    const s = loadTermSettings(st);
    expect(s.fontSize).toBe(DEFAULT_FONT);
    expect(s.theme).toBe("system");
  });
  it("字号档位边界（12 与 18 合法）", () => {
    const st = memStorage();
    for (const f of [FONT_SIZES[0], FONT_SIZES[FONT_SIZES.length - 1]]) {
      st.map.set("helm-console.term-font", String(f));
      expect(loadTermSettings(st).fontSize).toBe(f);
    }
  });
});

describe("resolveTermTheme", () => {
  it("显式档直取；system 跟随系统偏好", () => {
    const dark = resolveTermTheme("dark", false);
    const light = resolveTermTheme("light", true);
    expect(dark.background).toBe("#0d0d0d");
    expect(light.background).toBe("#fafafa");
    expect(resolveTermTheme("system", true).background).toBe("#0d0d0d");
    expect(resolveTermTheme("system", false).background).toBe("#fafafa");
  });
});

describe("saveTermSettings", () => {
  it("写入两条 key", () => {
    const st = memStorage();
    const settings: TermSettings = { fontSize: 12, theme: "dark" };
    saveTermSettings(settings, st);
    expect(st.map.get("helm-console.term-font")).toBe("12");
    expect(st.map.get("helm-console.term-theme")).toBe("dark");
  });
  it("storage 抛错安全吞掉（不 throw）", () => {
    expect(() =>
      saveTermSettings({ fontSize: 14, theme: "system" }, {
        setItem: () => {
          throw new Error("quota");
        },
      }),
    ).not.toThrow();
  });
});
