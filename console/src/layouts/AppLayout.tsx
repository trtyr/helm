import { useEffect, useState } from "react";
import { Link, Outlet, useLocation } from "react-router-dom";
import {
  Bell,
  ChevronLeft,
  Gauge,
  HardDrive,
  ListChecks,
  RadioTower,
  Settings,
  SunMoon,
  Waypoints,
} from "lucide-react";
import { useTheme } from "../hooks/useTheme";
import { ToastHost } from "../components/ToastHost";
import { NotificationBell } from "../components/NotificationBell";
import { clearChunkReloadFlag } from "../routes/placeholder";

/** 全局布局：Topbar 56 + Sidebar 220（可折叠 64）+ 内容 + StatusBar 28。 */

/**
 * 侧栏导航：`to` 为入口，`match` 为高亮路径组（合并菜单的子页面同组高亮，
 * 如「任务」覆盖 /jobs 与 /audit）。
 */
const NAV = [
  { to: "/dashboard", label: "仪表盘", icon: Gauge, match: ["/dashboard"] },
  { to: "/hosts", label: "主机", icon: HardDrive, match: ["/hosts"] },
  { to: "/notifications", label: "通知", icon: Bell, match: ["/notifications", "/alerts"] },
  { to: "/jobs", label: "任务", icon: ListChecks, match: ["/jobs", "/audit"] },
  { to: "/listeners", label: "监听器", icon: RadioTower, match: ["/listeners"] },
  { to: "/proxies", label: "代理", icon: Waypoints, match: ["/proxies"] },
] as const;

const SIDEBAR_KEY = "helm-console.sidebar";

export default function AppLayout() {
  const location = useLocation();
  const [collapsed, setCollapsed] = useState(
    () => localStorage.getItem(SIDEBAR_KEY) === "1",
  );
  const { toggle } = useTheme();

  // 应用正常渲染：解除 chunk 失败后的自动刷新守卫
  useEffect(() => {
    clearChunkReloadFlag();
  }, []);

  const toggleSidebar = () => {
    setCollapsed((c) => {
      localStorage.setItem(SIDEBAR_KEY, c ? "0" : "1");
      return !c;
    });
  };

  return (
    <div className="flex h-screen flex-col bg-background-100 text-gray-1000">
      {/* Topbar */}
      <header className="flex h-14 shrink-0 items-center gap-4 border-b border-gray-400 px-4">
        <Link to="/" className="flex items-center gap-2">
          <span className="flex h-6 w-6 items-center justify-center rounded-md bg-gray-1000 font-mono text-label-12 font-semibold text-background-100">
            ▲
          </span>
          <span className="text-label-14 font-medium">helm</span>
        </Link>
        <div className="mx-auto hidden h-8 w-72 items-center gap-2 rounded-md border border-gray-400 bg-gray-100 px-3 text-label-13 text-gray-900 md:flex">
          <span className="font-mono">⌘K</span>
          <span>搜索主机…</span>
          <span className="ml-auto text-label-12 text-gray-900">M2</span>
        </div>
        <div className="ml-auto flex items-center gap-1">
          <NotificationBell />
          <button
            type="button"
            onClick={toggle}
            aria-label="切换主题"
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <SunMoon size={16} strokeWidth={1.5} />
          </button>
          <Link
            to="/settings"
            aria-label="设置"
            className="flex h-8 w-8 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <Settings size={16} strokeWidth={1.5} />
          </Link>
          <span className="ml-2 flex h-6 w-6 items-center justify-center rounded-full bg-gray-700 text-label-12 font-medium">
            A
          </span>
        </div>
      </header>

      <div className="flex min-h-0 flex-1">
        {/* Sidebar */}
        <nav
          className={`flex shrink-0 flex-col border-r border-gray-400 transition-[width] duration-150 ${
            collapsed ? "w-16" : "w-55"
          }`}
        >
          <div className="flex flex-col gap-0.5 p-2">
            {NAV.map(({ to, label, icon: Icon, match }) => {
              const { pathname } = location;
              const active = match.some((m) => pathname === m || pathname.startsWith(`${m}/`));
              return (
                <Link
                  key={to}
                  to={to}
                  title={label}
                  className={`relative flex h-9 items-center gap-3 rounded-md px-3 text-label-14 transition-colors duration-150 ${
                    active
                      ? "bg-gray-200 text-gray-1000"
                      : "text-gray-900 hover:bg-gray-200 hover:text-gray-1000"
                  }`}
                >
                  {active && (
                    <span className="absolute left-0 top-1.5 h-6 w-0.5 rounded-full bg-blue-1000" />
                  )}
                  <Icon size={16} strokeWidth={1.5} className="shrink-0" />
                  {!collapsed && <span className="truncate">{label}</span>}
                </Link>
              );
            })}
          </div>
          <button
            type="button"
            onClick={toggleSidebar}
            aria-label={collapsed ? "展开侧栏" : "折叠侧栏"}
            className="mt-auto flex h-9 items-center justify-center rounded-md text-gray-900 transition-colors duration-150 hover:bg-gray-200 hover:text-gray-1000"
          >
            <ChevronLeft
              size={16}
              strokeWidth={1.5}
              className={`transition-transform duration-150 ${collapsed ? "rotate-180" : ""}`}
            />
          </button>
        </nav>

        {/* Content：max-w 1440 居中 + 左右 32px 留白（design-language 页面留白基准） */}
        <main className="min-w-0 flex-1 overflow-y-auto">
          <div className="mx-auto w-full max-w-[1440px] px-8 py-8">
            <Outlet />
          </div>
        </main>
      </div>

      {/* StatusBar */}
      <footer className="flex h-7 shrink-0 items-center gap-4 border-t border-gray-400 px-4 text-label-12 text-gray-900">
        <span className="flex items-center gap-1.5">
          <span className="h-1.5 w-1.5 rounded-full bg-amber-1000" />
          实时流：按页面订阅
        </span>
        <span className="ml-auto font-mono">helm-console v0.1.0</span>
      </footer>

      {/* 全局 toast 宿主 */}
      <ToastHost />
    </div>
  );
}
