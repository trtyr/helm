import type { ReactNode } from "react";
import type { SortState } from "../lib/useTableControls";

/**
 * 可排序表头（P001-T1）：点击循环 asc → desc → 取消；箭头指示当前状态。
 * 与数据行 <td> 使用同一套 padding（px-3），保证对齐。
 */
export function SortableTh({
  label,
  sortKey,
  sort,
  onSort,
  className = "",
  align = "",
}: {
  label: ReactNode;
  sortKey: string;
  sort: SortState;
  onSort: (key: string) => void;
  className?: string;
  align?: string;
}) {
  const active = sort?.key === sortKey;
  return (
    <th
      className={`whitespace-nowrap px-3 py-2.5 font-normal ${align} cursor-pointer select-none hover:text-gray-1000 ${className}`}
      onClick={() => onSort(sortKey)}
      title={active ? (sort!.desc ? "点击取消排序" : "点击降序") : "点击升序"}
    >
      {label} {active ? (sort!.desc ? "↓" : "↑") : ""}
    </th>
  );
}

/** 枚举筛选下拉的单项描述。 */
export interface ToolbarFilter {
  key: string;
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}

/**
 * 列表工具栏（P001-T1）：全局搜索框 + 枚举下拉组 + 右侧插槽。
 * 搜索框在左、筛选下拉随后、children 靠右（放批量操作等）。
 */
export function TableToolbar({
  search,
  onSearch,
  filters = [],
  children,
}: {
  search: string;
  onSearch: (v: string) => void;
  filters?: ToolbarFilter[];
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-2 px-4 py-2">
      <input
        value={search}
        onChange={(e) => onSearch(e.target.value)}
        placeholder="搜索…"
        className="w-56 rounded border border-gray-400 bg-gray-100 px-2.5 py-1.5 text-label-13 text-gray-1000 placeholder:text-gray-900 focus:border-gray-900 focus:outline-none"
      />
      {filters.map((f) => (
        <select
          key={f.key}
          value={f.value}
          onChange={(e) => f.onChange(e.target.value)}
          className="rounded border border-gray-400 bg-gray-100 px-2 py-1.5 text-label-13 text-gray-1000 focus:outline-none"
        >
          <option value="">{f.label}：全部</option>
          {f.options.map((o) => (
            <option key={o.value} value={o.value}>
              {f.label}：{o.label}
            </option>
          ))}
        </select>
      ))}
      <div className="ml-auto flex items-center gap-2">{children}</div>
    </div>
  );
}
