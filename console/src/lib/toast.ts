/** 全局 toast 队列（非组件文件，供任意处调用）。 */
export type ToastKind = "ok" | "success" | "warn" | "error";

export interface ToastItem {
  id: number;
  kind: ToastKind;
  text: string;
}

let items: ToastItem[] = [];
let listeners: ((l: ToastItem[]) => void)[] = [];
let seq = 0;

/** 弹出全局 toast（右下角，4s 自动消失，最多同时 5 条）。 */
export function toast(text: string, kind: ToastKind = "ok") {
  const item = { id: ++seq, kind, text };
  items = [...items, item].slice(-5);
  listeners.forEach((fn) => fn(items));
  setTimeout(() => {
    items = items.filter((i) => i.id !== item.id);
    listeners.forEach((fn) => fn(items));
  }, 4000);
}

/** 供 ToastHost 订阅队列变化。 */
export function subscribeToast(fn: (l: ToastItem[]) => void): () => void {
  listeners.push(fn);
  return () => {
    listeners = listeners.filter((f) => f !== fn);
  };
}

/** 当前队列快照。 */
export function currentToasts(): ToastItem[] {
  return items;
}
