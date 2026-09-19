/** JWT 本地解码（atob payload；无效 token 返回 null）。设置页账号/会话共用。 */
export function jwtExp(token: string): number | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1])) as { exp?: number };
    return typeof payload.exp === "number" ? payload.exp : null;
  } catch {
    return null;
  }
}

export function jwtSub(token: string): string | null {
  try {
    const payload = JSON.parse(atob(token.split(".")[1])) as { sub?: string };
    return payload.sub ?? null;
  } catch {
    return null;
  }
}
