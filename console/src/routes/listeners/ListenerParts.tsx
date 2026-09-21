// 监听器页的视图部件（G13 拆分，2026-09-21）——自 `Listeners.tsx` 拆出：
// 图标操作按钮与创建/编辑模态。
import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { components } from "../../api/schema";
import { api } from "../../api/client";
import { toast } from "../../lib/toast";

type Listener = components["schemas"]["Listener"];

export function OpBtn({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="flex h-7 w-7 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-30"
    >
      {children}
    </button>
  );
}

/** 创建/编辑模态（360px：名称/地址/协议禁用/专用 token 可选）。 */
export function ListenerFormModal({
  target,
  onClose,
  onSaved,
}: {
  target: Listener | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const editing = !!target;
  const [name, setName] = useState(target?.name ?? "");
  const [addr, setAddr] = useState(target?.addr ?? "");
  const [auth, setAuth] = useState("");

  const mutation = useMutation({
    mutationFn: () => {
      if (editing) {
        return api(`/api/v1/listeners/${target!.id}`, {
          method: "PUT",
          body: { name: name.trim(), addr: addr.trim(), proto: "grpc" },
        });
      }
      return api("/api/v1/listeners", {
        method: "POST",
        body: {
          name: name.trim(),
          addr: addr.trim(),
          proto: "grpc",
          ...(auth.trim() ? { auth: auth.trim() } : {}),
        },
      });
    },
    onSuccess: onSaved,
    onError: (e) => toast((e as Error).message, "error"),
  });

  const valid = name.trim().length > 0 && /^\S+:\d+$/.test(addr.trim());

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button type="button" aria-label="关闭" onClick={onClose} className="absolute inset-0 bg-black/40" />
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (valid) mutation.mutate();
        }}
        className="relative z-10 w-[360px] rounded-xl border border-gray-400 bg-background-100 p-6"
      >
        <h2 className="text-heading-16">{editing ? "编辑监听器" : "创建监听器"}</h2>

        <label htmlFor="ln-name" className="mt-4 block text-label-14">
          名称
        </label>
        <input
          id="ln-name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="内网备用"
          autoFocus
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />

        <label htmlFor="ln-addr" className="mt-4 block text-label-14">
          地址（ip:port）
        </label>
        <input
          id="ln-addr"
          value={addr}
          onChange={(e) => setAddr(e.target.value)}
          placeholder="0.0.0.0:50054"
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
        />

        <label htmlFor="ln-proto" className="mt-4 block text-label-14">
          协议
        </label>
        <input
          id="ln-proto"
          value="grpc"
          disabled
          className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 text-gray-900 opacity-60"
        />

        {!editing && (
          <>
            <label htmlFor="ln-auth" className="mt-4 block text-label-14">
              专用 token（可选）
            </label>
            <input
              id="ln-auth"
              type="password"
              value={auth}
              onChange={(e) => setAuth(e.target.value)}
              placeholder="留空则使用全局 HELM_SERVER_TOKEN"
              className="mt-2 h-8 w-full rounded-md border border-gray-400 bg-gray-100 px-3 font-mono text-label-14 outline-none transition-colors duration-150 hover:border-gray-500 focus-visible:border-gray-600"
            />
          </>
        )}

        {mutation.isError && (
          <p className="mt-3 text-label-13 text-red-1000" role="alert">
            ⚠ {(mutation.error as Error).message}
          </p>
        )}

        <div className="mt-6 flex justify-end gap-3">
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
