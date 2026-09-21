// 自启动页的视图部件（G13 拆分，2026-09-21）——自 `Autostart.tsx` 拆出：
// 分类标签 Pill 与单条发现行（含 diff 高亮、禁用/删除操作）。
import type { IrFinding } from "./shared";
import { SEV_CLS, SEV_LABEL, SIGN_CLS, SIGN_LABEL, fmtTime } from "./shared";

export function Pill({
  label,
  count,
  active,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`flex h-7 items-center gap-1 rounded-full border px-2.5 text-label-13 transition-colors ${
        active
          ? "border-gray-900 bg-gray-900 text-white"
          : "border-gray-500 text-gray-900 hover:bg-gray-200"
      }`}
    >
      {label}
      <span className={`font-mono text-label-12 ${active ? "text-white/70" : "text-gray-900/50"}`}>{count}</span>
    </button>
  );
}

/** 单条自启动发现：级别 / 条目 / 发布者 / 签名 / 文件时间 / 路径 / 操作。 */
export function FindingRow({
  f,
  ds,
  actionPending,
  onAction,
}: {
  f: IrFinding;
  ds?: "added" | "removed";
  actionPending: boolean;
  onAction: (action: "disable" | "enable" | "delete", f: IrFinding) => void;
}) {
  return (
    <tr
      title={f.detail}
      className={`border-b border-gray-400/60 transition-colors duration-150 last:border-0 hover:bg-gray-100 ${
        ds === "added"
          ? "bg-green-1000/5"
          : ds === "removed"
            ? "bg-red-1000/5"
            : f.disabled
              ? "opacity-50"
              : ""
      }`}
    >
      <td className="px-3 py-2">
        <span className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${SEV_CLS[f.severity ?? "info"] ?? SEV_CLS.info}`}>
          {SEV_LABEL[f.severity ?? "info"] ?? "信息"}
        </span>
      </td>
      <td className="px-3 py-2">
        <div className="truncate text-label-13 text-gray-900" title={f.name}>
          {ds === "added" && <span className="mr-1 text-green-1000">▲</span>}
          {ds === "removed" && <span className="mr-1 text-red-1000">▼</span>}
          {f.name}
        </div>
        {(f.desc || f.disabled) && (
          <div className="truncate text-label-12 text-gray-900/60">
            {f.disabled && <span className="mr-1 text-amber-1000">[已禁用]</span>}
            <span title={f.desc ?? ""}>{f.desc}</span>
          </div>
        )}
      </td>
      <td className="px-3 py-2">
        <span className="block truncate text-label-12 text-gray-900" title={f.publisher ?? ""}>
          {f.publisher ?? "—"}
        </span>
      </td>
      <td className="px-3 py-2">
        {f.signState ? (
          <span className={`whitespace-nowrap rounded px-1.5 py-0.5 text-label-12 ${SIGN_CLS[f.signState] ?? SIGN_CLS.unknown}`}>
            {SIGN_LABEL[f.signState] ?? f.signState}
          </span>
        ) : (
          <span className="text-label-12 text-gray-900/40">—</span>
        )}
      </td>
      <td className="whitespace-nowrap px-3 py-2 text-label-12 text-gray-900">
        {fmtTime(f.mtime)}
      </td>
      <td className="max-w-0 px-4 py-2">
        <span className="block truncate font-mono text-label-12 text-gray-900" title={f.path ?? ""}>
          {f.path ?? "—"}
        </span>
      </td>
      <td className="whitespace-nowrap px-3 py-2">
        <div className="flex items-center gap-1.5">
          {f.opKey && (
            <>
              <button
                type="button"
                title={f.disabled ? "启用（移出 AutorunsDisabled）" : "禁用（移入 AutorunsDisabled）"}
                disabled={actionPending}
                onClick={() => onAction(f.disabled ? "enable" : "disable", f)}
                className="rounded border border-gray-500 px-1.5 py-0.5 text-label-12 text-gray-900 hover:bg-gray-200 disabled:opacity-40"
              >
                {f.disabled ? "启用" : "禁用"}
              </button>
              <button
                type="button"
                title="删除该自启动项"
                disabled={actionPending}
                onClick={() => onAction("delete", f)}
                className="rounded border border-red-1000/40 px-1.5 py-0.5 text-label-12 text-red-1000 hover:bg-red-1000/10 disabled:opacity-40"
              >
                删除
              </button>
            </>
          )}
        </div>
      </td>
    </tr>
  );
}
