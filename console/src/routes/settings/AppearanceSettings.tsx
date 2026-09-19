import { useTheme } from "../../hooks/useTheme";

/** /settings/appearance：外观——主题切换（默认暗色，跟随本浏览器持久化）。 */
export default function AppearanceSettings() {
  const { theme, toggle } = useTheme();
  return (
    <section className="rounded-lg border border-gray-400 p-6">
      <h2 className="text-heading-16">外观</h2>
      <div className="mt-4 flex items-center gap-4">
        <span className="text-label-14">主题</span>
        <div className="flex rounded-md border border-gray-500 p-0.5">
          <button
            type="button"
            onClick={() => theme === "light" && toggle()}
            aria-pressed={theme === "dark"}
            className={`h-7 rounded px-4 text-label-13 transition-colors duration-150 ${
              theme === "dark" ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
            }`}
          >
            ● 暗色
          </button>
          <button
            type="button"
            onClick={() => theme === "dark" && toggle()}
            aria-pressed={theme === "light"}
            className={`h-7 rounded px-4 text-label-13 transition-colors duration-150 ${
              theme === "light" ? "bg-gray-200 text-gray-1000" : "text-gray-900 hover:text-gray-1000"
            }`}
          >
            ○ 亮色
          </button>
        </div>
      </div>
      <p className="mt-2 text-label-12 text-gray-900">默认暗色，跟随本浏览器持久化</p>
    </section>
  );
}
