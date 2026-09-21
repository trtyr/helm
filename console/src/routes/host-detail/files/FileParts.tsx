// 文件浏览页的模态与队列部件（G13 拆分，2026-09-21）——自 `Files.tsx` 拆出：
// 路径对模态（上传/下载共用）与右下传输队列卡片。
import { useState } from "react";
import type { Transfer } from "./shared";

/** 路径对模态（上传/下载共用）。 */
export function PathPairModal({
  title,
  hint,
  fromLabel,
  fromPlaceholder,
  fromDefault,
  fromDisabled,
  toLabel,
  toPlaceholder,
  toDefault,
  submitLabel,
  submitting,
  error,
  onClose,
  onSubmit,
}: {
  title: string;
  hint: string;
  fromLabel: string;
  fromPlaceholder?: string;
  fromDefault?: string;
  fromDisabled?: boolean;
  toLabel: string;
  toPlaceholder?: string;
  toDefault?: string;
  submitLabel: string;
  submitting: boolean;
  error: string | null;
  onClose: () => void;
  onSubmit: (from: string, to: string) => void;
}) {
  const [from, setFrom] = useState(fromDefault ?? "");
  const [to, setTo] = useState(toDefault ?? "");

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (from.trim() && to.trim()) onSubmit(from.trim(), to.trim());
        }}
        className="relative z-10 w-[420px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16">{title}</h2>
        <p className="mt-2 text-label-12 text-gray-900">{hint}</p>
        <label className="mt-4 block text-label-14" htmlFor="pair-from">
          {fromLabel}
        </label>
        <input
          id="pair-from"
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          placeholder={fromPlaceholder}
          disabled={fromDisabled || submitting}
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        <label className="mt-4 block text-label-14" htmlFor="pair-to">
          {toLabel}
        </label>
        <input
          id="pair-to"
          value={to}
          onChange={(e) => setTo(e.target.value)}
          placeholder={toPlaceholder}
          disabled={submitting}
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
        {error && (
          <p className="mt-3 text-label-13 text-red-1000" role="alert">
            ⚠ {error}
          </p>
        )}
        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={submitting}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={submitting || !from.trim() || !to.trim()}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {submitting ? "传输中…" : submitLabel}
          </button>
        </div>
      </form>
    </div>
  );
}

/** 右下固定传输队列卡片（进行中 / 完成 / 失败 + 校验结果）。 */
export function TransferQueue({
  transfers,
  onDismiss,
}: {
  transfers: Transfer[];
  onDismiss: (id: number) => void;
}) {
  if (transfers.length === 0) return null;
  return (
    <div className="fixed bottom-10 right-4 z-40 w-96 rounded-xl border border-gray-400 bg-background-100 p-3 shadow-lg">
      <p className="text-label-13 text-gray-900">传输队列</p>
      <div className="mt-2 flex flex-col gap-1.5">
        {transfers.map((t) => (
          <div key={t.id} className="flex items-center gap-2 text-label-12">
            <span className={t.direction === "upload" ? "text-blue-1000" : "text-teal-1000"}>
              {t.direction === "upload" ? "↑" : "↓"}
            </span>
            <span className="min-w-0 flex-1 truncate font-mono text-gray-900">{t.label}</span>
            {t.state === "running" && (
              <span className="h-3 w-3 animate-spin rounded-full border border-gray-900 border-t-gray-1000" />
            )}
            {t.state === "done" && (
              <span className={t.checksumOk ? "text-green-1000" : "text-red-1000"}>
                {t.checksumOk ? "✓ 校验通过" : "✗ 校验失败"}
              </span>
            )}
            {t.state === "failed" && (
              <span className="truncate text-red-1000" title={t.error}>
                ✗ 失败
              </span>
            )}
            {t.state !== "running" && (
              <button
                type="button"
                aria-label="移除"
                onClick={() => onDismiss(t.id)}
                className="text-gray-900 hover:text-gray-1000"
              >
                ×
              </button>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}
