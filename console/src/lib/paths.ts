/** 远端路径工具（POSIX 语义，纯函数便于测试）。 */

/** 拼接：joinPath("/var/log", "nginx") → "/var/log/nginx"；根目录拼接不加多余斜杠。 */
export function joinPath(base: string, name: string): string {
  if (base === "/" || base === "") return `/${name}`;
  return `${base}/${name}`;
}

/** 父目录：parentPath("/var/log") → "/var"；parentPath("/var") → "/"；根返回 null。 */
export function parentPath(path: string): string | null {
  if (path === "/" || path === "") return null;
  const idx = path.lastIndexOf("/");
  return idx <= 0 ? "/" : path.slice(0, idx);
}

/** 面包屑段：crumbSegments("/var/log/nginx") → ["/", "/var", "/var/log", "/var/log/nginx"]。 */
export function crumbSegments(path: string): string[] {
  if (path === "/" || path === "") return ["/"];
  const parts = path.split("/").filter(Boolean);
  return ["/", ...parts.map((_, i) => `/${parts.slice(0, i + 1).join("/")}`)];
}

/** 段显示名：段路径 → 名称（根显示 /）。 */
export function crumbLabel(segment: string): string {
  return segment === "/" ? "/" : segment.split("/").filter(Boolean).pop() ?? "/";
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
