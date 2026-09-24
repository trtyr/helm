import { NavLink, Outlet } from "react-router-dom";

/**
 * /logs 布局（P003 T5：日志中心）——左侧分类二级导航 + 右侧内容。
 * 任务/审计/状态事件统一入口（owner：三类同属日志，不该分散在顶级菜单）。
 */

const NAV = [
  { to: "/logs/jobs", label: "任务日志", desc: "执行历史与状态" },
  { to: "/logs/audit", label: "审计日志", desc: "谁做了什么" },
  { to: "/logs/events", label: "状态事件", desc: "上下线与断连原因" },
] as const;

export default function LogsLayout() {
  return (
    <div className="mx-auto flex w-full max-w-[1080px] flex-col gap-6">
      <div>
        <h1 className="text-heading-24">日志</h1>
        <p className="mt-1 text-copy-13 text-gray-900">排查根基——任务、审计与状态事件的统一入口</p>
      </div>
      <div className="flex items-start gap-8">
        {/* 左侧分类二级导航（当前域高亮，同 Settings 模式） */}
        <nav className="flex w-48 shrink-0 flex-col gap-0.5" aria-label="日志分类">
          {NAV.map(({ to, label, desc }) => (
            <NavLink
              key={to}
              to={to}
              className={({ isActive }) =>
                `rounded-md px-3 py-2 transition-colors duration-150 ${
                  isActive ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:bg-gray-200/60 hover:text-gray-1000"
                }`
              }
            >
              <span className="block text-label-14">{label}</span>
              <span className="block text-label-12 text-gray-900">{desc}</span>
            </NavLink>
          ))}
        </nav>
        {/* 右侧内容（每域一页） */}
        <div className="min-w-0 flex-1">
          <Outlet />
        </div>
      </div>
    </div>
  );
}
