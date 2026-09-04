/** 审计资源列摘要（前端推导：detail 首个标识性字段，规格 audit.md）。 */
export function auditResource(detail: Record<string, unknown> | undefined | null): string {
  if (!detail) return "—";
  for (const key of ["hostname", "agent_id", "job_id", "service", "name", "listener_id", "path", "command"]) {
    const v = detail[key];
    if (typeof v === "string" && v.length > 0) {
      return v.length > 24 ? `${v.slice(0, 24)}…` : v;
    }
  }
  const first = Object.values(detail)[0];
  if (typeof first === "string") return first.slice(0, 24);
  return "—";
}
