import { Link } from "react-router-dom";

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
