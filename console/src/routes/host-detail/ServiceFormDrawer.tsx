import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api } from "../../api/client";

type Service = components["schemas"]["Service"];

/** 创建/编辑服务抽屉（规格 services.md F41：400px 右抽屉）。 */
export default function ServiceFormDrawer({
  service,
  agentId,
  onClose,
  onSaved,
}: {
  service: Service | null;
  agentId: string | undefined;
  onClose: () => void;
  onSaved: (service: Service | null) => void;
}) {
  const editing = !!service;
  const [name, setName] = useState(service?.name ?? "");
  const [command, setCommand] = useState(service?.command ?? "");
  const [argsText, setArgsText] = useState((service?.args ?? []).join("\n"));
  const [policy, setPolicy] = useState<"no" | "always">(service?.restart_policy ?? "no");

  const mutation = useMutation({
    mutationFn: async () => {
      const args = argsText.split("\n").map((l) => l.trim()).filter(Boolean);
      if (editing) {
        return api(`/api/v1/services/${service!.id}`, {
          method: "PUT",
          body: { name: name.trim(), command: command.trim(), args, restart_policy: policy },
        }).then((r) => (r as { service?: Service }).service ?? null);
      }
      return api("/api/v1/services", {
        method: "POST",
        body: {
          agent_id: agentId,
          name: name.trim(),
          command: command.trim(),
          args,
          restart_policy: policy,
        },
      }).then((r) => (r as { service?: Service }).service ?? null);
    },
    onSuccess: (svc) => onSaved(svc),
  });

  const valid = name.trim().length > 0 && command.trim().length > 0 && (editing || !!agentId);

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
        <h2 className="text-heading-20">{editing ? "编辑服务" : "创建服务"}</h2>

        <div>
          <label htmlFor="svc-name" className="block text-label-14">
            名称
          </label>
          <input
            id="svc-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="nginx"
            autoFocus
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <label htmlFor="svc-command" className="block text-label-14">
            命令
          </label>
          <input
            id="svc-command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="/usr/sbin/nginx"
            className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <label htmlFor="svc-args" className="block text-label-14">
            参数（每行一个）
          </label>
          <textarea
            id="svc-args"
            value={argsText}
            onChange={(e) => setArgsText(e.target.value)}
            placeholder={"-g\ndaemon off;"}
            rows={4}
            className="mt-2 w-full resize-y rounded-md border border-gray-400 bg-gray-100 px-3 py-2 font-mono text-label-13 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
          />
        </div>

        <div>
          <span className="block text-label-14">重启策略</span>
          <div className="mt-2 flex rounded-md border border-gray-500 p-0.5">
            {(["no", "always"] as const).map((p) => (
              <button
                key={p}
                type="button"
                onClick={() => setPolicy(p)}
                className={`h-7 flex-1 rounded text-label-13 transition-colors duration-150 ${
                  policy === p ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
                }`}
              >
                {p === "no" ? "不重启" : "始终重启"}
              </button>
            ))}
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
            {mutation.isPending ? "保存中…" : editing ? "保存" : "创建"}
          </button>
        </div>
      </form>
    </div>
  );
}
