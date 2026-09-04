import { Navigate, createBrowserRouter } from "react-router-dom";
import AppLayout from "./layouts/AppLayout";
import { RequireAuth } from "./layouts/RequireAuth";
import { NotFound } from "./routes/placeholder";

/** 路由树（决策 003）：/login 公开；受保护路由挂在 AppLayout 下。 */
export const router = createBrowserRouter([
  {
    path: "/login",
    lazy: async () => {
      const { default: Login } = await import("./routes/login/Login");
      return { Component: Login };
    },
  },
  {
    element: <RequireAuth />,
    children: [
      {
        path: "/",
        element: <AppLayout />,
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
                path: "metrics",
                lazy: async () => {
                  const { default: Metrics } = await import("./routes/host-detail/Metrics");
                  return { Component: Metrics };
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
              const { default: Jobs } = await import("./routes/jobs/Jobs");
              return { Component: Jobs };
            },
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
              const { default: Notifications } = await import("./routes/Notifications");
              return { Component: Notifications };
            },
          },
          {
            path: "alerts",
            lazy: async () => {
              const { default: Alerts } = await import("./routes/Alerts");
              return { Component: Alerts };
            },
          },
          {
            path: "audit",
            lazy: async () => {
              const { default: Audit } = await import("./routes/Audit");
              return { Component: Audit };
            },
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
          {
            path: "forward",
            lazy: async () => {
              const { default: Forward } = await import("./routes/Forward");
              return { Component: Forward };
            },
          },
          { path: "*", element: <NotFound /> },
        ],
      },
    ],
  },
]);
