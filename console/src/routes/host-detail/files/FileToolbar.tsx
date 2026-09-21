// 文件浏览页工具行（G13 拆分，2026-09-21）——自 `Files.tsx` 拆出：
// 面包屑、路径跳转输入、返回上级、刷新、上传入口。
import { ArrowUp, ChevronRight, CornerDownLeft, RefreshCw, Upload } from "lucide-react";
import { crumbLabel, crumbSegments, parentPath } from "../../../lib/paths";

export function FileToolbar({
  path,
  onPath,
  jumpDraft,
  onJumpDraft,
  isFetching,
  onRefresh,
  canUpload,
  onUpload,
}: {
  path: string;
  onPath: (p: string) => void;
  jumpDraft: string | null;
  onJumpDraft: (v: string | null) => void;
  isFetching: boolean;
  onRefresh: () => void;
  canUpload: boolean;
  onUpload: () => void;
}) {
  const jumpValue = jumpDraft ?? path;
  return (
    <div className="flex flex-wrap items-center gap-2">
      <div className="flex min-w-0 flex-1 items-center gap-2">
        {/* 面包屑 */}
        <nav
          className="hidden min-w-0 flex-wrap items-center gap-0.5 font-mono text-label-13 md:flex"
          aria-label="路径"
        >
          {crumbSegments(path).map((seg, i, arr) => (
            <span key={seg} className="flex items-center">
              {i > 0 && <ChevronRight size={12} strokeWidth={1.5} className="text-gray-900" />}
              <button
                type="button"
                onClick={() => onPath(seg)}
                className={`rounded px-1 py-0.5 transition-colors duration-150 hover:bg-gray-200 ${
                  i === arr.length - 1 ? "text-gray-1000" : "text-blue-1000"
                }`}
              >
                {crumbLabel(seg)}
              </button>
            </span>
          ))}
        </nav>
        {/* 路径跳转输入框：回车直达任意路径 */}
        <form
          className="relative ml-auto flex min-w-0 flex-1 items-center md:ml-2 md:max-w-md"
          onSubmit={(e) => {
            e.preventDefault();
            if (jumpDraft !== null) onPath(jumpDraft.trim());
            onJumpDraft(null);
          }}
        >
          <input
            value={jumpValue}
            onChange={(e) => onJumpDraft(e.target.value)}
            onBlur={() => onJumpDraft(null)}
            placeholder="输入路径回车跳转，如 C:\Windows 或 /var/log"
            aria-label="路径跳转"
            spellCheck={false}
            className="h-8 w-full rounded-md border border-gray-400 bg-gray-100 pl-3 pr-8 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
          <button
            type="submit"
            aria-label="跳转"
            title="跳转"
            className="absolute right-2 top-1/2 -translate-y-1/2 text-gray-900 hover:text-gray-1000"
          >
            <CornerDownLeft size={13} strokeWidth={1.5} />
          </button>
        </form>
      </div>
      <button
        type="button"
        aria-label="返回上级"
        disabled={parentPath(path) === null}
        onClick={() => onPath(parentPath(path) ?? "")}
        className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
      >
        <ArrowUp size={14} strokeWidth={1.5} />
      </button>
      <button
        type="button"
        aria-label="刷新"
        onClick={onRefresh}
        className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
      >
        <RefreshCw size={14} strokeWidth={1.5} className={isFetching ? "animate-spin" : ""} />
      </button>
      <button
        type="button"
        onClick={onUpload}
        disabled={!canUpload}
        className="flex h-8 items-center gap-1.5 rounded-md border border-gray-500 px-3 text-label-13 transition-colors duration-150 hover:bg-gray-200 disabled:opacity-40"
      >
        <Upload size={14} strokeWidth={1.5} />
        上传
      </button>
    </div>
  );
}
