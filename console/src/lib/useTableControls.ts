import { useMemo, useState } from "react";

/**
 * 列表基座：筛选/排序统一控制（P001-T1 轻量三层）。
 *
 * 三层能力：
 *  ① 列头点击排序——asc → desc → 取消 三态循环（null = 回默认序）；
 *  ② 全局搜索框——searchText(row) 拼接的文本做 substring 匹配；
 *  ③ 枚举列下拉筛选——columns 里声明 enumOptions/matchesEnum 的列。
 *
 * 设计约束（roadmap 设计决议②）：不做每列复杂筛选器；全量返回的列表
 * 在前端完成全部计算；服务端分页列表的排序走 API 下沉（sort 参数），
 * 本 hook 仅用于前端持有全量数据的列表。
 */

/** 排序状态：null = 未排序（保持原始顺序）。 */
export type SortState = { key: string; desc: boolean } | null;

export interface TableColumn<T> {
  key: string;
  /** 排序取值（数值按数值比较、其余按字符串比较、null 恒排尾部）。缺省 = 该列不可排序。 */
  value?: (row: T) => string | number | null | undefined;
  /** 枚举筛选候选项（页面从数据派生）。缺省 = 该列无下拉筛选。 */
  enumOptions?: () => { value: string; label: string }[];
  /** 行是否命中该列筛选值。 */
  matchesEnum?: (row: T, value: string) => boolean;
}

export interface TableControlsOptions<T> {
  columns: TableColumn<T>[];
  /** 全局搜索的文本源（把参与搜索的列拼成一个串）。 */
  searchText?: (row: T) => string;
  /** 默认排序（如 { key: "cpu", desc: true }）。 */
  defaultSort?: { key: string; desc: boolean };
}

export function useTableControls<T>(rows: T[], opts: TableControlsOptions<T>) {
  const [sort, setSort] = useState<SortState>(opts.defaultSort ?? null);
  const [search, setSearch] = useState("");
  const [enumFilters, setEnumFilters] = useState<Record<string, string>>({});

  /** 列头点击：换列 = 升序；同列 = asc → desc → 取消。 */
  const toggleSort = (key: string) => {
    setSort((s) => {
      if (!s || s.key !== key) return { key, desc: false };
      return s.desc ? null : { key, desc: true };
    });
  };

  const setEnumFilter = (key: string, value: string) =>
    setEnumFilters((m) => ({ ...m, [key]: value }));

  const visible = useMemo(() => {
    let out = rows;
    const q = search.trim().toLowerCase();
    if (q) out = out.filter((r) => (opts.searchText?.(r) ?? "").toLowerCase().includes(q));
    for (const [k, v] of Object.entries(enumFilters)) {
      if (!v) continue;
      const col = opts.columns.find((c) => c.key === k);
      if (col?.matchesEnum) out = out.filter((r) => col.matchesEnum!(r, v));
    }
    if (sort) {
      const col = opts.columns.find((c) => c.key === sort.key);
      if (col?.value) {
        out = [...out].sort((a, b) => {
          const va = col.value!(a);
          const vb = col.value!(b);
          if (va == null && vb == null) return 0;
          if (va == null) return 1; // null 恒排尾部，不随方向翻转
          if (vb == null) return -1;
          const dir = sort.desc ? -1 : 1;
          if (typeof va === "number" && typeof vb === "number") return (va - vb) * dir;
          return String(va).localeCompare(String(vb)) * dir;
        });
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rows, search, enumFilters, sort]);

  return { search, setSearch, sort, toggleSort, enumFilters, setEnumFilter, visible };
}
