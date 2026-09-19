import { useState } from "react";

const PAGE_SIZES = [20, 50, 100] as const;

/**
 * 列表基座（P001-T4）：服务端分页底栏。
 * 三件套：页大小选择 + 页码跳转 + 总页数/总条数（total 由 API 提供）。
 * `onPatch` 收 `{page?, limit?}`——null 值表示从 URL params 删除（回第一页语义由调用方保证）。
 */
export function PaginationBar({
  page,
  limit,
  total,
  onPatch,
}: {
  page: number;
  limit: number;
  total: number;
  onPatch: (patch: Record<string, string | null>) => void;
}) {
  const [jump, setJump] = useState("");
  const totalPages = Math.max(1, Math.ceil(total / limit));

  const goto = (raw: string) => {
    const p = Math.min(totalPages, Math.max(1, Number(raw) || 1));
    onPatch({ page: String(p) });
    setJump("");
  };

  return (
    <div className="flex h-9 items-center justify-between border-t border-gray-400 px-4 font-mono text-label-13 text-gray-900">
      <span className="flex items-center gap-3 whitespace-nowrap">
        <span>
          共 {total} 条 · 第 {page}/{totalPages} 页
        </span>
        <label className="flex items-center gap-1">
          每页
          <select
            aria-label="每页数量"
            value={String(limit)}
            onChange={(e) => onPatch({ limit: e.target.value, page: null })}
            className="h-6 rounded border border-gray-400 bg-background-100 px-1"
          >
            {PAGE_SIZES.map((n) => (
              <option key={n} value={n}>
                {n}
              </option>
            ))}
          </select>
          条
        </label>
      </span>
      <span className="flex items-center gap-3">
        <button
          type="button"
          disabled={page <= 1}
          onClick={() => onPatch({ page: String(page - 1) })}
          className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
        >
          ‹ 上一页
        </button>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            goto(jump);
          }}
          className="flex items-center gap-1"
        >
          <input
            aria-label="跳转页码"
            value={jump}
            onChange={(e) => setJump(e.target.value.replace(/\D/g, ""))}
            placeholder={String(page)}
            className="h-6 w-12 rounded border border-gray-400 bg-background-100 px-1 text-center"
          />
          <span>/ {totalPages} 页</span>
          <button
            type="submit"
            className="rounded border border-gray-400 px-1.5 transition-colors duration-150 hover:text-gray-1000"
          >
            跳转
          </button>
        </form>
        <button
          type="button"
          disabled={page >= totalPages}
          onClick={() => onPatch({ page: String(page + 1) })}
          className="transition-colors duration-150 hover:text-gray-1000 disabled:opacity-30"
        >
          下一页 ›
        </button>
      </span>
    </div>
  );
}
