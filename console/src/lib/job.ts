/** Job 状态展示（后端枚举：running / succeeded / failed；queued 前端容错）。 */
export interface JobStatusMeta {
  label: string;
  cls: string;
}

export function jobStatusMeta(status: string | undefined): JobStatusMeta {
  switch (status) {
    case "succeeded":
      return { label: "✓ 成功", cls: "text-green-1000" };
    case "failed":
      return { label: "✗ 失败", cls: "text-red-1000" };
    case "running":
      return { label: "● 运行中", cls: "text-blue-1000" };
    case "queued":
      return { label: "○ 排队中", cls: "text-gray-900" };
    default:
      return { label: status ?? "—", cls: "text-gray-900" };
  }
}

/** 任务类型徽标（task_id 有无推导，规格 host-tasks.md）。 */
export function jobKindLabel(taskId: string | null | undefined): string {
  return taskId ? "定时" : "快速";
}

/** 间隔秒数 → 人类可读（规格 host-tasks.md）。 */
export function intervalLabel(secs: number): string {
  if (secs % 86_400 === 0) return `每 ${secs / 86_400}d`;
  if (secs % 3_600 === 0) return `每 ${secs / 3_600}h`;
  if (secs % 60 === 0) return `每 ${secs / 60}m`;
  return `每 ${secs}s`;
}
