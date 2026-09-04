import { useState } from "react";

/** 删除主机确认模态（规格：输入 hostname 二次确认，danger 按钮）。 */
export function DeleteHostDialog({
  hostname,
  onClose,
  onConfirm,
  submitting,
}: {
  hostname: string | null;
  onClose: () => void;
  onConfirm: () => void;
  submitting: boolean;
}) {
  const [confirmText, setConfirmText] = useState("");

  if (!hostname) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="absolute inset-0 bg-black/40"
      />
      <div
        role="dialog"
        aria-label="删除主机"
        className="relative z-10 w-[360px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16 text-red-1000">删除主机</h2>
        <p className="mt-3 text-copy-13 text-gray-900">
          此操作不可恢复。删除后其下 Agent 记录与历史数据将一并移除。
        </p>
        <p className="mt-3 text-label-13">
          输入主机名 <span className="font-mono text-gray-1000">{hostname}</span> 以确认：
        </p>
        <input
          value={confirmText}
          onChange={(e) => setConfirmText(e.target.value)}
          disabled={submitting}
          placeholder={hostname}
          autoFocus
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />
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
            type="button"
            onClick={onConfirm}
            disabled={submitting || confirmText !== hostname}
            className="h-8 rounded-md bg-red-700 px-4 text-label-14 text-white transition-colors duration-150 hover:bg-red-1000 disabled:opacity-40"
          >
            {submitting ? "删除中…" : "删除"}
          </button>
        </div>
      </div>
    </div>
  );
}
