import { useState } from "react";

export interface HostFormValues {
  hostname: string;
  conn_mode: "reverse" | "forward";
  addr: string;
  tags: string[];
}

/** 编辑主机右抽屉（规格 routes/hosts.md）：400px、conn_mode 分段、addr 条件显示。
 * 主机不在此创建（由 Agent 上线自动注册）。调用方以 `key={host.id}` 挂载，保证每次打开重置表单状态。 */
export function HostFormDrawer({
  open,
  initial,
  onClose,
  onSubmit,
  submitting,
  error,
}: {
  open: boolean;
  initial?: Partial<HostFormValues>;
  onClose: () => void;
  onSubmit: (values: HostFormValues) => void;
  submitting: boolean;
  error?: string | null;
}) {
  const [hostname, setHostname] = useState(initial?.hostname ?? "");
  const [mode, setMode] = useState<"reverse" | "forward">(initial?.conn_mode ?? "reverse");
  const [addr, setAddr] = useState(initial?.addr ?? "");
  const [tags, setTags] = useState<string[]>(initial?.tags ?? []);
  const [tagDraft, setTagDraft] = useState("");

  if (!open) return null;

  const addTag = () => {
    const v = tagDraft.trim();
    if (v && !tags.includes(v)) setTags([...tags, v]);
    setTagDraft("");
  };

  return (
    <div className="fixed inset-0 z-40 flex justify-end">
      <button
        type="button"
        aria-label="关闭"
        onClick={onClose}
        className="absolute inset-0 bg-black/40"
      />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onSubmit({ hostname: hostname.trim(), conn_mode: mode, addr: addr.trim(), tags });
        }}
        className="relative z-10 flex h-full w-[400px] flex-col gap-6 overflow-y-auto border-l border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-20">编辑主机</h2>

        <div>
          <label className="block text-label-14" htmlFor="hostname">
            主机名 <span className="text-red-1000">*</span>
          </label>
          <input
            id="hostname"
            required
            value={hostname}
            onChange={(e) => setHostname(e.target.value)}
            disabled={submitting}
            autoFocus
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <span className="block text-label-14">连接模式</span>
          <div className="mt-2 grid grid-cols-2 gap-2">
            {(["reverse", "forward"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => setMode(m)}
                disabled={submitting}
                className={`h-8 rounded-md border text-label-13 transition-colors duration-150 ${
                  mode === m
                    ? "border-blue-1000 bg-blue-1000/10 text-blue-1000"
                    : "border-gray-400 text-gray-900 hover:border-gray-500"
                }`}
              >
                {m === "reverse" ? "反向（Agent 主动连）" : "正向（Server 拨号）"}
              </button>
            ))}
          </div>
        </div>

        {mode === "forward" && (
          <div>
            <label className="block text-label-13 text-gray-900" htmlFor="addr">
              拨号地址（正向必填）
            </label>
            <input
              id="addr"
              placeholder="ip:50052"
              value={addr}
              onChange={(e) => setAddr(e.target.value)}
              disabled={submitting}
              className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
          </div>
        )}

        <div>
          <label className="block text-label-14" htmlFor="tag-input">
            标签
          </label>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {tags.map((t) => (
              <span
                key={t}
                className="flex items-center gap-1 rounded border border-gray-400 bg-gray-200 px-1.5 py-0.5 text-label-12"
              >
                {t}
                <button
                  type="button"
                  aria-label={`移除 ${t}`}
                  className="text-gray-900 transition-colors duration-150 hover:text-red-1000"
                  onClick={() => setTags(tags.filter((x) => x !== t))}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
          <input
            id="tag-input"
            value={tagDraft}
            onChange={(e) => setTagDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addTag();
              }
            }}
            onBlur={addTag}
            disabled={submitting}
            placeholder="输入后回车添加"
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        {error && (
          <p className="text-label-13 text-red-1000" role="alert">
            ⚠ {error}
          </p>
        )}

        <div className="mt-auto flex justify-end gap-3">
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
            disabled={submitting}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {submitting ? "保存中…" : "保存"}
          </button>
        </div>
      </form>
    </div>
  );
}
