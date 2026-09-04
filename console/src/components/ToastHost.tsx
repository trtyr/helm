import { useEffect, useState } from "react";
import { currentToasts, subscribeToast, type ToastItem } from "../lib/toast";

/** 全局 toast 宿主：AppLayout 挂载一次，右下角渲染队列（Geist 克制风）。 */
export function ToastHost() {
  const [list, setList] = useState<ToastItem[]>(currentToasts);
  useEffect(() => subscribeToast(setList), []);
  if (list.length === 0) return null;
  return (
    <div className="pointer-events-none fixed bottom-10 right-4 z-[60] flex flex-col gap-2">
      {list.map((t) => (
        <div
          key={t.id}
          role="status"
          className={`rounded-lg border border-gray-400 bg-background-100 px-3 py-2 text-label-13 shadow-lg ${
            t.kind === "ok" ? "text-gray-1000" : t.kind === "warn" ? "text-amber-1000" : "text-red-1000"
          }`}
        >
          {t.kind === "ok" ? "✓ " : t.kind === "warn" ? "⚠ " : "✗ "}
          {t.text}
        </div>
      ))}
    </div>
  );
}
