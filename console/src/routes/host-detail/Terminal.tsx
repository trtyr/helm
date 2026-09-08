import { useState } from "react";
import { useOutletContext } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Maximize2, Plus, Settings2 } from "lucide-react";
import type { components } from "../../api/schema";
import { api, pickAgent } from "../../api/client";
import { TerminalSession } from "./TerminalSession";
import {
  FONT_SIZES,
  TERM_THEMES,
  loadTermSettings,
  saveTermSettings,
  type TermThemeName,
} from "../../lib/termSettings";

type HostView = components["schemas"]["HostView"];
type Agent = components["schemas"]["Agent"];

interface Ctx {
  host: HostView;
}

/** /hosts/:id/terminal（规格 routes/terminal.md）：多会话页签 + 设置（字号/主题）+ 全屏 + 满高终端。 */
export default function Terminal() {
  const { host } = useOutletContext<Ctx>();
  const [sessions, setSessions] = useState<number[]>([1]);
  const [nextId, setNextId] = useState(2);
  const [active, setActive] = useState(1);
  const [settings, setSettings] = useState(() => loadTermSettings(localStorage));
  const [settingsOpen, setSettingsOpen] = useState(false);

  const agentsQuery = useQuery({
    queryKey: ["agents"],
    queryFn: () => api<{ agents: Agent[] }>("/api/v1/agents"),
    refetchInterval: 30_000,
  });
  const agent = pickAgent(agentsQuery.data?.agents ?? [], host.id);

  function patchSettings(patch: Partial<{ fontSize: number; theme: TermThemeName }>) {
    setSettings((prev) => {
      const next = { ...prev, ...patch };
      saveTermSettings(next, localStorage);
      return next;
    });
  }

  function closeSession(id: number) {
    setSessions((list) => {
      const next = list.filter((s) => s !== id);
      if (next.length === 0) next.push(Date.now()); // 关最后一 tab 自动开新会话
      if (active === id) setActive(next[0]);
      return next;
    });
  }

  if (!agent) {
    return (
      <div className="flex flex-col items-center justify-center rounded-lg border border-dashed border-gray-500 py-16">
        <p className="text-copy-13 text-gray-900">
          该主机还没有在线 Agent——Agent 注册后可使用终端
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-[calc(100vh-56px-28px-220px)] min-h-[480px] flex-col gap-3">
      {/* 会话条 */}
      <div className="flex items-center gap-1">
        {sessions.map((id) => (
          <span
            key={id}
            className={`flex h-8 items-center gap-2 rounded-md border px-3 text-label-13 transition-colors duration-150 ${
              active === id
                ? "border-gray-500 bg-gray-200 text-gray-1000"
                : "border-gray-400 text-gray-900 hover:bg-gray-100"
            }`}
          >
            <button type="button" onClick={() => setActive(id)}>
              sh {id}
            </button>
            <button
              type="button"
              aria-label={`关闭会话 ${id}`}
              onClick={() => closeSession(id)}
              className="text-gray-900 transition-colors duration-150 hover:text-red-1000"
            >
              ×
            </button>
          </span>
        ))}
        <button
          type="button"
          aria-label="新会话"
          onClick={() => {
            setSessions((l) => [...l, nextId]);
            setActive(nextId);
            setNextId((n) => n + 1);
          }}
          disabled={sessions.length >= 4}
          title={sessions.length >= 4 ? "会话较多，注意主机负载" : undefined}
          className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000 disabled:opacity-40"
        >
          <Plus size={14} strokeWidth={1.5} />
        </button>
        <div className="relative ml-auto flex items-center gap-1">
          <button
            type="button"
            aria-label="全屏终端"
            title="全屏（Esc 退出）"
            onClick={() => {
              const el = document.getElementById("term-area");
              if (document.fullscreenElement) void document.exitFullscreen();
              else void el?.requestFullscreen?.();
            }}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <Maximize2 size={16} strokeWidth={1.5} />
          </button>
          <button
            type="button"
            aria-label="终端设置"
            onClick={() => setSettingsOpen((v) => !v)}
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <Settings2 size={16} strokeWidth={1.5} />
          </button>
          {settingsOpen && (
            <>
              <button
                type="button"
                aria-label="关闭设置"
                onClick={() => setSettingsOpen(false)}
                className="fixed inset-0 z-20 cursor-default"
              />
              <div className="absolute right-0 top-9 z-30 w-56 rounded-xl border border-gray-400 bg-background-100 p-3 shadow-lg">
                <p className="text-label-13 text-gray-900">字号</p>
                <div className="mt-2 flex gap-1">
                  {FONT_SIZES.map((s) => (
                    <button
                      key={s}
                      type="button"
                      onClick={() => patchSettings({ fontSize: s })}
                      className={`h-7 flex-1 rounded-md border text-label-12 transition-colors duration-150 ${
                        settings.fontSize === s
                          ? "border-blue-1000 bg-blue-1000/10 text-blue-1000"
                          : "border-gray-400 text-gray-900 hover:border-gray-500"
                      }`}
                    >
                      {s}
                    </button>
                  ))}
                </div>
                <p className="mt-3 text-label-13 text-gray-900">主题（仅终端）</p>
                <div className="mt-2 flex flex-col gap-1">
                  {TERM_THEMES.map((t) => (
                    <button
                      key={t.value}
                      type="button"
                      onClick={() => patchSettings({ theme: t.value })}
                      className={`h-7 rounded-md border px-2 text-left text-label-12 transition-colors duration-150 ${
                        settings.theme === t.value
                          ? "border-blue-1000 bg-blue-1000/10 text-blue-1000"
                          : "border-gray-400 text-gray-900 hover:border-gray-500"
                      }`}
                    >
                      {t.label}
                    </button>
                  ))}
                </div>
                <p className="mt-3 text-label-12 text-gray-900">
                  行高固定 1.2；字号即时生效（重建会话）
                </p>
              </div>
            </>
          )}
        </div>
      </div>

      {/* 会话实例（非激活 display:none 保持连接） */}
      <div id="term-area" className="flex min-h-0 flex-1 flex-col gap-3 bg-background-100">
        {sessions.map((id) => (
          <TerminalSession
            key={id}
            agentId={agent.id ?? ""}
            fontSize={settings.fontSize}
            theme={settings.theme}
            active={active === id}
            onExited={() => closeSession(id)}
          />
        ))}
      </div>
    </div>
  );
}
