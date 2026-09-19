import { NavLink, Outlet } from "react-router-dom";

/**
 * /settings 布局（roadmap T8：GitHub Settings 式）——左侧分类二级导航 + 右侧内容。
 * 每域一页路由 /settings/<domain>，可收藏可直达。
 */

const NAV = [
  { to: "/settings/account", label: "账号", desc: "用户名、密码" },
  { to: "/settings/api-keys", label: "API 密钥", desc: "供 AI / 脚本调用" },
  { to: "/settings/appearance", label: "外观", desc: "主题" },
  { to: "/settings/sessions", label: "会话", desc: "登录状态" },
  { to: "/settings/system", label: "系统", desc: "版本与 Agent 安装" },
] as const;

export default function SettingsLayout() {
  return (
    <div className="mx-auto flex w-full max-w-[880px] flex-col gap-6">
      <div>
        <h1 className="text-heading-24">设置</h1>
        <p className="mt-1 text-copy-13 text-gray-900">管理账号、凭证与控制台行为</p>
      </div>
      <div className="flex items-start gap-8">
        {/* 左侧分类二级导航（当前域高亮） */}
        <nav className="flex w-48 shrink-0 flex-col gap-0.5" aria-label="设置分类">
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
