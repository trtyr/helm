import { Link, useRouteError } from "react-router-dom";

/** 404（规格 routes/README：heading-64 + 返回首页链接）。 */
export function NotFound() {
  return (
    <div className="flex flex-col items-center justify-center py-24">
      <h1 className="font-mono text-[4rem] leading-none tracking-tight text-gray-1000">404</h1>
      <p className="mt-4 text-copy-14 text-gray-900">页面不存在</p>
      <Link
        to="/"
        className="mt-6 h-8 rounded-md border border-gray-500 px-4 text-label-13 leading-8 transition-colors duration-150 hover:bg-gray-200"
      >
        返回首页
      </Link>
    </div>
  );
}

const CHUNK_RELOAD_KEY = "helm-console.chunk-reloaded";

/** 应用成功渲染后调用：清除自动刷新守卫（下次 chunk 失败仍可自动恢复）。 */
export function clearChunkReloadFlag() {
  sessionStorage.removeItem(CHUNK_RELOAD_KEY);
}

/** 路由级错误边界：懒加载 chunk 失败（发版后旧页面请求已被删除/换名的模块）
 * 自动整页刷新一次；其余错误展示可重试的友好界面。 */
export function RouteError() {
  const error = useRouteError() as Error;

  const isChunkError =
    error instanceof TypeError &&
    /dynamically imported module|Importing a module script failed|error loading dynamically imported/i.test(
      error.message ?? "",
    );

  if (isChunkError && !sessionStorage.getItem(CHUNK_RELOAD_KEY)) {
    sessionStorage.setItem(CHUNK_RELOAD_KEY, "1");
    window.location.reload();
    return null;
  }

  return (
    <div className="flex flex-col items-center justify-center py-24">
      <h1 className="font-mono text-[4rem] leading-none tracking-tight text-gray-1000">:(</h1>
      <p className="mt-4 text-copy-14 text-gray-900">
        页面加载失败——系统可能刚完成升级，刷新后即可恢复
      </p>
      {error?.message && (
        <p className="mt-2 max-w-xl truncate font-mono text-label-12 text-gray-900" title={error.message}>
          {error.message}
        </p>
      )}
      <div className="mt-6 flex items-center gap-3">
        <button
          type="button"
          onClick={() => window.location.reload()}
          className="h-8 rounded-md bg-gray-700 px-4 text-label-13 transition-colors duration-150 hover:bg-gray-800"
        >
          刷新页面
        </button>
        <Link
          to="/"
          className="h-8 rounded-md border border-gray-500 px-4 text-label-13 leading-8 transition-colors duration-150 hover:bg-gray-200"
        >
          返回首页
        </Link>
      </div>
    </div>
  );
}
