import { Link, Outlet, useLocation } from "react-router-dom";

const TABS = [
  { to: "/notifications", label: "通知" },
  { to: "/alerts", label: "告警" },
] as const;

/** 通知中心布局：通知 / 告警 双子页（同一菜单，URL 保持 /notifications 与 /alerts）。 */
export default function NotificationsLayout() {
  const { pathname } = useLocation();
  return (
    <div className="flex flex-col gap-5">
      <nav className="flex items-center gap-6 border-b border-gray-400">
        {TABS.map(({ to, label }) => {
          const active = pathname === to;
          return (
            <Link
              key={to}
              to={to}
              className={`relative -mb-px flex h-10 items-center text-label-14 transition-colors duration-150 ${
                active ? "text-gray-1000" : "text-gray-900 hover:text-gray-1000"
              }`}
            >
              {label}
              {active && <span className="absolute inset-x-0 bottom-0 h-0.5 rounded-full bg-blue-1000" />}
            </Link>
          );
        })}
      </nav>
      <Outlet />
    </div>
  );
}
