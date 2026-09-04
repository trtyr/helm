/**
 * localStorage polyfill（vitest setup）。
 * Node ≥25 自带的实验性 Web Storage 与 vitest 4 DOM 环境注入冲突，
 * globalThis.localStorage 可能缺失——这里兜底一个最小实现，测试不依赖环境注入。
 */
import { beforeEach } from "vitest";

if (typeof globalThis.localStorage === "undefined" || globalThis.localStorage === null) {
  const store = new Map<string, string>();
  const storage: Storage = {
    getItem: (k) => store.get(k) ?? null,
    setItem: (k, v) => void store.set(k, String(v)),
    removeItem: (k) => void store.delete(k),
    clear: () => store.clear(),
    key: (i) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(globalThis, "localStorage", {
    value: storage,
    configurable: true,
    writable: true,
  });
}

beforeEach(() => {
  globalThis.localStorage?.clear();
});
