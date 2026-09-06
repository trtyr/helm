#!/usr/bin/env python3
"""任务域：脚本下发与定时任务。

用法：
  python tasks.py script <agent_id> --sh "echo hi && uname -a"        # bash -c
  python tasks.py script <agent_id> --ps "Get-Process | Select -First 3"  # powershell -Command
  python tasks.py script <agent_id> --command bash --args -c,"echo hi"  # 原样下发
  python tasks.py schedule <agent_id> --interval 60 --sh "df -h"      # 每 60s 执行

script 返回 job_id（用 exec.py wait / job 查结果）。
定时任务落库且 Server 重启自动恢复；当前平台未提供列出/删除定时任务的端点。
"""

import argparse

import common


def cmd_script(a):
    if a.sh is None and a.ps is None and not a.command:
        common.die("需要 --sh / --ps / --command 之一")
    if a.sh is not None:
        command, args = "bash", ["-c", a.sh]
    elif a.ps is not None:
        command, args = "powershell", ["-NoProfile", "-Command", a.ps]
    else:
        command, args = a.command, common.parse_list(a.args)
    job_id = common.call("POST", "/tasks/script",
                         {"agent_id": a.agent_id, "command": command, "args": args})["job_id"]
    if not a.wait:
        common.output({"job_id": job_id})
        return
    import exec as exec_mod
    job = exec_mod.wait_job(job_id, a.timeout)
    common.output({
        "job_id": job_id,
        "status": job.get("status"),
        "exit_code": job.get("exit_code"),
        "output": job.get("output") or "",
        "error": job.get("error"),
    })


def cmd_schedule(a):
    if a.sh is not None:
        command, args = "bash", ["-c", a.sh]
    elif a.ps is not None:
        command, args = "powershell", ["-NoProfile", "-Command", a.ps]
    elif a.command:
        command, args = a.command, common.parse_list(a.args)
    else:
        common.die("需要 --sh / --ps / --command 之一")
    common.output(common.call("POST", "/tasks/schedule", {
        "agent_id": a.agent_id, "command": command, "args": args,
        "interval_secs": a.interval,
    }))


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    sc = sub.add_parser("script", help="下发脚本（等价一次 exec）")
    sc.add_argument("agent_id")
    sc.add_argument("--sh", help="bash -c 字符串")
    sc.add_argument("--ps", help="powershell -Command 字符串")
    sc.add_argument("--command", help="原样程序名")
    sc.add_argument("--args", help="逗号分隔参数（配合 --command）")
    sc.add_argument("--wait", action="store_true", help="顺带等待 job 终态")
    sc.add_argument("--timeout", type=int, default=120)
    common.add_common_args(sc)

    sd = sub.add_parser("schedule", help="创建定时任务")
    sd.add_argument("agent_id")
    sd.add_argument("--interval", type=int, required=True, help="间隔秒数")
    sd.add_argument("--sh")
    sd.add_argument("--ps")
    sd.add_argument("--command")
    sd.add_argument("--args")
    common.add_common_args(sd)

    a = p.parse_args()
    common.apply_common_args(a)
    {"script": cmd_script, "schedule": cmd_schedule}[a.cmd](a)


if __name__ == "__main__":
    main()
