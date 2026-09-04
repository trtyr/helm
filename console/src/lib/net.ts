/** 网络接口类型推断（前端静态映射，规格 network.md：未识别不标注）。 */
export function ifaceKind(name: string): string | null {
  if (name === "lo" || name.startsWith("lo")) return "回环接口";
  if (/^(eth|enp|eno|ens|wlan)/.test(name)) return "常规网络接口";
  if (/^(docker|veth|br-?|vmnet|bridge)/.test(name)) return "容器 / 网桥";
  if (/^utun/.test(name)) return "隧道接口";
  if (/^(llw|awdl)/.test(name)) return "本地无线接力";
  return null;
}
