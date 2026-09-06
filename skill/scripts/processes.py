#!/usr/bin/env python3
"""进程与网络域：目标机进程列表、杀进程、网络接口信息。

用法：
  python processes.py ps <agent_id> [--raw]
  python processes.py kill <agent_id> --pid 1234
  python processes.py net <agent_id>
"""

import argparse

import common


def cmd_ps(a):
    data = common.call("POST", "/processes/list", {"agent_id": a.agent_id})
    if a.raw:
        for proc in data.get("processes", []):
            print(f"{proc['pid']:>8}  {proc['cpu_percent']:>6.1f}%  "
                  f"{proc['mem_bytes']:>12}  {proc['name']}")
    else:
        common.output(data)


def cmd_kill(a):
    common.output(common.call("POST", "/processes/kill",
                              {"agent_id": a.agent_id, "pid": a.pid}))


def cmd_net(a):
    common.output(common.call("POST", "/net/info", {"agent_id": a.agent_id}))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    ps = sub.add_parser("ps", help="列出进程")
    ps.add_argument("agent_id")
    ps.add_argument("--raw", action="store_true", help="简洁表格输出")
    common.add_common_args(ps)

    kl = sub.add_parser("kill", help="杀进程（pid 不存在返回 ok=false）")
    kl.add_argument("agent_id")
    kl.add_argument("--pid", type=int, required=True)
    common.add_common_args(kl)

    nt = sub.add_parser("net", help="网络接口信息")
    nt.add_argument("agent_id")
    common.add_common_args(nt)

    a = p.parse_args()
    common.apply_common_args(a)
    {"ps": cmd_ps, "kill": cmd_kill, "net": cmd_net}[a.cmd](a)


if __name__ == "__main__":
    main()
