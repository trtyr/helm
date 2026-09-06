#!/usr/bin/env python3
"""正向连接域：Server 主动拨号 forward 模式 Agent 执行命令（按需拨号）。

用法：
  python forward.py exec --hostname web-forward -- uname -a
  python forward.py exec --addr 203.0.113.7:50052 -- cat /etc/hostname

`--` 之后原样作为远端命令（本地选项写在 `--` 之前）。
--hostname 需主机为 forward 模式且已配 addr；--addr 直接指定拨号地址。
注意：forward 持久连接（forward_manager 调和）注册后的主机可直接用 exec.py。
"""

import argparse

import common


def cmd_exec(a):
    if not a.hostname and not a.addr:
        common.die("需要 --hostname 或 --addr 之一")
    if not a.command:
        common.die("需要 `--` 分隔的命令与参数")
    body = {"command": a.command[0], "args": list(a.command[1:])}
    if a.hostname:
        body["hostname"] = a.hostname
    if a.addr:
        body["agent_addr"] = a.addr
    data = common.call("POST", "/forward/exec", body)
    if a.raw:
        print(data.get("output", ""), end="")
        raise SystemExit(data.get("exit_code") or 0)
    common.output(data)
    if data.get("exit_code"):
        raise SystemExit(data["exit_code"])


def main():
    pre, post = common.split_dashdash()

    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("exec", help="正向连接执行命令")
    e.add_argument("--hostname", help="按主机名解析（forward 模式）")
    e.add_argument("--addr", help="直接拨号地址 host:port")
    e.add_argument("--raw", action="store_true", help="只打印输出")
    common.add_common_args(e)

    a = p.parse_args(pre)
    common.apply_common_args(a)
    a.command = post
    cmd_exec(a)


if __name__ == "__main__":
    main()
