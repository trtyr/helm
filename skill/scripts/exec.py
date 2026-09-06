#!/usr/bin/env python3
"""命令执行域：下发命令、Job 查询、阻塞等待结果。

用法：
  python exec.py run [选项] <agent_id> -- <command> [args...]     # 阻塞到结束
  python exec.py submit [选项] <agent_id> -- <command> [args...]  # 只提交，打印 job_id
  python exec.py job <job_id>                                     # 查询单个 Job
  python exec.py jobs [--page 1 --limit 20]
  python exec.py wait <job_id> [--timeout 60]                     # 轮询到终态

`--` 之后的参数原样传给目标机（本地选项必须写在 `--` 之前）。
run 退出码 = 命令退出码（succeeded）；failed→1、timed_out→124、cancelled→130。
"""

import argparse
import sys
import time

import common

TERMINAL = {"succeeded", "failed", "timed_out", "cancelled"}
EXIT_MAP = {"failed": 1, "timed_out": 124, "cancelled": 130}


def wait_job(job_id, timeout=60, interval=0.5):
    deadline = time.time() + timeout
    while True:
        job = common.call("GET", f"/jobs/{job_id}")["job"]
        if job.get("status") in TERMINAL or time.time() > deadline:
            return job
        time.sleep(interval)


def cmd_run(a):
    if not a.command:
        common.die("需要 `--` 分隔的命令与参数（选项写在 `--` 之前）")
    agent_id, command, args = a.agent_id, a.command[0], list(a.command[1:])
    body = {"agent_id": agent_id, "command": command, "args": args}
    if a.timeout_secs:
        body["timeout_secs"] = a.timeout_secs
    if a.working_dir:
        body["working_dir"] = a.working_dir
    job_id = common.call("POST", "/exec", body)["job_id"]
    if a.submit_only:
        if a.raw:
            print(job_id)
        else:
            common.output({"job_id": job_id})
        return
    job = wait_job(job_id, a.timeout)
    result = {
        "job_id": job_id,
        "status": job.get("status"),
        "exit_code": job.get("exit_code"),
        "output": job.get("output") or "",
        "error": job.get("error"),
    }
    if a.raw:
        print(result["output"], end="")
    else:
        common.output(result)
    if result["status"] == "succeeded":
        raise SystemExit(result["exit_code"] or 0)
    raise SystemExit(EXIT_MAP.get(result["status"], 1))


def cmd_job(a):
    common.output(common.call("GET", f"/jobs/{a.id}"))


def cmd_jobs(a):
    common.output(common.call("GET", "/jobs", query={"page": a.page, "limit": a.limit}))


def cmd_wait(a):
    common.output(wait_job(a.id, a.timeout))


def main():
    pre, post = common.split_dashdash()

    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    run = sub.add_parser("run", help="下发并等待完成")
    run.add_argument("agent_id")
    run.add_argument("--timeout", type=int, default=120, help="等待终态秒数（默认 120）")
    run.add_argument("--timeout-secs", type=int, help="传给 Agent 的命令超时")
    run.add_argument("--working-dir")
    run.add_argument("--raw", action="store_true", help="只打印命令输出")
    run.add_argument("--submit-only", action="store_true", help="只提交不等待")
    common.add_common_args(run)

    sb = sub.add_parser("submit", help="只提交，打印 job_id")
    sb.add_argument("agent_id")
    sb.add_argument("--timeout-secs", type=int)
    sb.add_argument("--working-dir")
    sb.add_argument("--raw", action="store_true", help="只打印裸 job_id（便于管道）")
    common.add_common_args(sb)

    sub.add_parser("job", help="查询 Job").add_argument("id")
    jb = sub.add_parser("jobs", help="分页列出 Job")
    jb.add_argument("--page", type=int, default=1)
    jb.add_argument("--limit", type=int, default=20)
    common.add_common_args(jb)
    wt = sub.add_parser("wait", help="轮询等待 Job 终态")
    wt.add_argument("id")
    wt.add_argument("--timeout", type=int, default=60)
    common.add_common_args(wt)

    a = p.parse_args(pre)
    common.apply_common_args(a)
    if a.cmd == "run":
        a.command = post
    elif a.cmd == "submit":
        a.command = post
        a.submit_only = True
    {"run": cmd_run, "submit": cmd_run, "job": cmd_job,
     "jobs": cmd_jobs, "wait": cmd_wait}[a.cmd](a)


if __name__ == "__main__":
    main()
