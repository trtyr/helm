// 定时任务创建抽屉（G13 拆分，2026-09-21）——自 `Tasks.tsx` 拆出。
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { api } from "../../../api/client";
import { intervalLabel } from "../../../lib/job";
import { toast } from "../../../lib/toast";

/** 定时任务创建抽屉（F30：命令 + 参数 + 间隔预设 chips + 自定义秒数）。 */
export function ScheduleDrawer({
  agentId,
  onClose,
}: {
  agentId: string | undefined;
  onClose: () => void;
}) {
  const PRESETS = [60, 300, 3_600, 21_600, 86_400];
  const [command, setCommand] = useState("");
  const [argsText, setArgsText] = useState("");
  const [interval, setIntervalSecs] = useState(3_600);

  const mutation = useMutation({
    mutationFn: () =>
      api("/api/v1/tasks/schedule", {
        method: "POST",
        body: {
          agent_id: agentId,
          command: command.trim(),
          args: argsText.split("\n").map((l) => l.trim()).filter(Boolean),
          interval_secs: interval,
        },
      }),
    onSuccess: () => {
      toast("已创建，Server 重启后自动恢复");
      onClose();
    },
    onError: (e) => toast((e as Error).message, "error"),
  });

  const valid = command.trim().length > 0 && !!agentId && interval > 0;

  return (
    <div className="fixed inset-0 z-50">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) mutation.mutate();
        }}
        className="absolute right-0 top-0 flex h-full w-[400px] flex-col gap-5 overflow-y-auto border-l border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-20">定时任务</h2>

        <div>
          <label htmlFor="task-command" className="block text-label-14">
            命令
          </label>
          <input
            id="task-command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="/usr/local/bin/clean.sh"
            autoFocus
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <label htmlFor="task-args" className="block text-label-14">
            参数（每行一个）
          </label>
          <textarea
            id="task-args"
            value={argsText}
            onChange={(e) => setArgsText(e.target.value)}
            rows={3}
            className="mt-2 w-full resize-y rounded-md border border-gray-400 bg-gray-100 px-3 py-2 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <span className="block text-label-14">间隔</span>
          <div className="mt-2 flex flex-wrap gap-2">
            {PRESETS.map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setIntervalSecs(p)}
                className={`h-7 rounded-full border px-3 font-mono text-label-12 transition-colors duration-150 ${
                  interval === p ? "border-gray-1000 bg-gray-200 text-gray-1000" : "border-gray-500 text-gray-900 hover:border-gray-600"
                }`}
              >
                {intervalLabel(p)}
              </button>
            ))}
          </div>
          <div className="mt-3 flex items-center gap-2">
            <input
              type="number"
              min={1}
              value={interval}
              onChange={(e) => setIntervalSecs(Math.max(1, Number(e.target.value) || 0))}
              aria-label="自定义间隔秒数"
              className="h-8 w-28 rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
            <span className="text-label-13 text-gray-900">秒（自定义）</span>
          </div>
        </div>

        {mutation.isError && (
          <p className="text-label-13 text-red-1000" role="alert">
            ⚠ {(mutation.error as Error).message}
          </p>
        )}

        <div className="mt-auto flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={mutation.isPending}
            className="h-8 rounded-md border border-gray-500 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-200"
          >
            取消
          </button>
          <button
            type="submit"
            disabled={!valid || mutation.isPending}
            className="h-8 rounded-md bg-gray-700 px-4 text-label-14 transition-colors duration-150 hover:bg-gray-800 disabled:opacity-50"
          >
            {mutation.isPending ? "创建中…" : "创建"}
          </button>
        </div>
      </form>
    </div>
  );
}
