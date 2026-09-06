import { Navigate, createBrowserRouter } from "react-router-dom";
import AppLayout from "./layouts/AppLayout";
import { RequireAuth } from "./layouts/RequireAuth";
import { NotFound, RouteError } from "./routes/placeholder";

/** 路由树（决策 003）：/login 公开；受保护路由挂在 AppLayout 下。 */
export const router = createBrowserRouter([
  {
    path: "/login",
    lazy: async () => {
      const { default: Login } = await import("./routes/login/Login");
      return { Component: Login };
    },
    errorElement: <RouteError />,
  },
  {
    element: <RequireAuth />,
    errorElement: <RouteError />,
    children: [
      {
        path: "/",
        element: <AppLayout />,
        errorElement: <RouteError />,
        children: [
          { index: true, element: <Navigate to="/dashboard" replace /> },
          {
            path: "dashboard",
            lazy: async () => {
              const { default: Dashboard } = await import("./routes/Dashboard");
              return { Component: Dashboard };
            },
          },
          {
            path: "hosts",
            lazy: async () => {
              const { default: Hosts } = await import("./routes/hosts/Hosts");
              return { Component: Hosts };
            },
          },
          {
            path: "hosts/:id",
            lazy: async () => {
              const { default: HostDetailLayout } = await import(
                "./routes/host-detail/HostDetailLayout"
              );
              return { Component: HostDetailLayout };
            },
            children: [
              { index: true, element: <Navigate to="overview" replace /> },
              {
                path: "overview",
                lazy: async () => {
                  const { default: Overview } = await import("./routes/host-detail/Overview");
                  return { Component: Overview };
                },
              },
              {
                path: "terminal",
                lazy: async () => {
                  const { default: Terminal } = await import("./routes/host-detail/Terminal");
                  return { Component: Terminal };
                },
              },
              {
                path: "files",
                lazy: async () => {
                  const { default: Files } = await import("./routes/host-detail/Files");
                  return { Component: Files };
                },
              },
              {
                path: "services",
                lazy: async () => {
                  const { default: Services } = await import("./routes/host-detail/Services");
                  return { Component: Services };
                },
              },
              {
                path: "processes",
                lazy: async () => {
                  const { default: Processes } = await import("./routes/host-detail/Processes");
                  return { Component: Processes };
                },
              },
          {
            path: "network",
            lazy: async () => {
              const { default: Network } = await import("./routes/host-detail/Network");
              return { Component: Network };
            },
          },
          {
            path: "tasks",
                lazy: async () => {
                  const { default: Tasks } = await import("./routes/host-detail/Tasks");
                  return { Component: Tasks };
                },
              },
            ],
          },
          {
            path: "jobs",
            lazy: async () => {
              const { default: JobsLayout } = await import("./routes/jobs/JobsLayout");
              return { Component: JobsLayout };
            },
            children: [
              { index: true, lazy: async () => {
                const { default: Jobs } = await import("./routes/jobs/Jobs");
                return { Component: Jobs };
              } },
              { path: "audit", lazy: async () => {
                const { default: Audit } = await import("./routes/Audit");
                return { Component: Audit };
              } },
            ],
          },
          {
            path: "jobs/:id",
            lazy: async () => {
              const { default: JobDetail } = await import("./routes/jobs/JobDetail");
              return { Component: JobDetail };
            },
          },
          {
            path: "notifications",
            lazy: async () => {
              const { default: NotificationsLayout } = await import(
                "./routes/notifications/NotificationsLayout"
              );
              return { Component: NotificationsLayout };
            },
            children: [
              { index: true, lazy: async () => {
                const { default: Notifications } = await import("./routes/Notifications");
                return { Component: Notifications };
              } },
              { path: "alerts", lazy: async () => {
                const { default: Alerts } = await import("./routes/Alerts");
                return { Component: Alerts };
              } },
            ],
          },
          // 旧 URL 兼容重定向（Dashboard 等历史链接）
          { path: "alerts", element: <Navigate to="/notifications/alerts" replace /> },
          {
            path: "audit",
            element: <Navigate to="/jobs/audit" replace />,
          },
          {
            path: "listeners",
            lazy: async () => {
              const { default: Listeners } = await import("./routes/Listeners");
              return { Component: Listeners };
            },
          },
          {
            path: "settings",
            lazy: async () => {
              const { default: Settings } = await import("./routes/Settings");
              return { Component: Settings };
            },
          },
          { path: "*", element: <NotFound /> },
        ],
      },
    ],
  },
]);
