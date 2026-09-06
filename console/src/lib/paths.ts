/**
 * 远端路径工具（POSIX + Windows 双语义，纯函数便于测试）。
 *
 * Windows 根视图：空串 "" 表示「此电脑」（驱动器列表）；驱动器路径如 `C:\Users`。
 * POSIX 根："/"。
 */

/** 是否 Windows 风格路径（含盘符或反斜杠）。 */
export function isWinPath(path: string): boolean {
  if (!path) return false;
  return /^[A-Za-z]:/.test(path) || path.includes("\\");
}

/** 拼接：joinPath("/var/log", "nginx") → "/var/log/nginx"；joinPath("C:\\Users", "pub") → "C:\\Users\\pub"。 */
export function joinPath(base: string, name: string): string {
  // 此电脑根 → 驱动器名自带分隔符（"C:\"），直接作为新路径
  if (base === "") return name;
  if (isWinPath(base) || /^[A-Za-z]:/.test(name)) {
    return base.endsWith("\\") ? base + name : `${base}\\${name}`;
  }
  if (base === "/") return `/${name}`;
  return `${base}/${name}`;
}

/** 父目录：parentPath("C:\\Users\\pub") → "C:\\Users"；驱动器根 → ""（此电脑）；根返回 null。 */
export function parentPath(path: string): string | null {
  if (path === "" || path === "/") return null;
  if (isWinPath(path)) {
    // 驱动器根（如 "C:\"）的上一级是此电脑
    if (/^[A-Za-z]:[\\/]?$/.test(path)) return "";
    const idx = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
    // "C:\x" 的分隔符在索引 2 → 上一级是驱动器根 "C:\"
    if (idx <= 2) return path.slice(0, 3);
    return path.slice(0, idx);
  }
  const idx = path.lastIndexOf("/");
  return idx <= 0 ? "/" : path.slice(0, idx);
}

/** 面包屑段：crumbSegments("C:\\Users\\pub") → ["", "C:\\", "C:\\Users", "C:\\Users\\pub"]。 */
export function crumbSegments(path: string): string[] {
  if (path === "" || path === "/") return [path];
  if (!isWinPath(path)) {
    const parts = path.split("/").filter(Boolean);
    return ["/", ...parts.map((_, i) => `/${parts.slice(0, i + 1).join("/")}`)];
  }
  // 归一为反斜杠后逐级拼接；仅驱动器段（首段）带尾分隔符
  const norm = path.replace(/\//g, "\\");
  const parts = norm.split("\\").filter(Boolean);
  return [
    "",
    ...parts.map((_, i) => {
      const joined = parts.slice(0, i + 1).join("\\");
      return i === 0 ? `${joined}\\` : joined;
    }),
  ];
}

/** 段显示名：段路径 → 名称（"" 显示 此电脑）。 */
export function crumbLabel(segment: string): string {
  if (segment === "") return "此电脑";
  if (segment === "/") return "/";
  if (isWinPath(segment)) {
    const parts = segment.replace(/\\$/, "").split("\\").filter(Boolean);
    return parts[parts.length - 1] ?? segment;
  }
  return segment.split("/").filter(Boolean).pop() ?? "/";
}

/** 字节数人类可读：humanSize(1536) → "1.5 KB"。 */
export function humanSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes;
  let i = -1;
  do {
    v /= 1024;
    i++;
  } while (v >= 1024 && i < units.length - 1);
  return `${v.toFixed(1)} ${units[i]}`;
}
