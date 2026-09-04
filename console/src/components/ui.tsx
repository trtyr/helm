import type { ReactNode } from "react";

/** 在线状态徽标（F14）：在线 green 实心 / 离线 gray 空心 / stale amber 半透明。 */
export function StatusDot({ online, stale }: { online: boolean; stale?: boolean }) {
  const title = online ? "在线" : stale ? "心跳超时" : "离线";
  const cls = online
    ? "bg-green-1000"
    : stale
      ? "bg-amber-1000 opacity-50"
      : "border border-gray-900";
  return (
    <span className="flex items-center gap-2" title={title}>
      <span className={`inline-block h-2 w-2 shrink-0 rounded-full ${cls}`} />
    </span>
  );
}

/** 表格骨架行。 */
export function SkeletonRows({ rows = 8, cols }: { rows?: number; cols: number }) {
  return (
    <>
      {Array.from({ length: rows }).map((_, i) => (
        <tr key={i} className="animate-pulse">
          {Array.from({ length: cols }).map((_, j) => (
            <td key={j} className="px-4 py-3">
              <div
                className="h-3.5 rounded bg-gray-200"
                style={{ width: j === 0 ? "40%" : `${30 + ((i + j) % 4) * 15}%` }}
              />
            </td>
          ))}
        </tr>
      ))}
    </>
  );
}

/** 页内错误卡（通用状态基线：非模态、可重试、不白屏）。 */
export function ErrorCard({
  title = "加载失败",
  detail,
  onRetry,
}: {
  title?: string;
  detail?: string;
  onRetry?: () => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-gray-400 py-16">
      <p className="text-label-14 text-red-1000">{title}</p>
      {detail && <p className="mt-1 font-mono text-label-12 text-gray-900">{detail}</p>}
      {onRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="mt-4 h-8 rounded-md border border-gray-500 bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800"
        >
          重试
        </button>
      )}
    </div>
  );
}

/** 空态（居中说明 + 可选操作）。 */
export function EmptyState({ message, action }: { message: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-gray-500 py-16">
      <p className="text-copy-13 text-gray-900">{message}</p>
      {action && <div className="mt-4">{action}</div>}
    </div>
  );
}
