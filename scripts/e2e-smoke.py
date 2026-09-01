#!/usr/bin/env python3
"""一键端到端 smoke：Postgres + Server + Agent → 登录 → 下发命令 → 验证结果。"""

import os
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    GRPC_ADDR,
    HTTP_ADDR,
    http_json,
    login,
    start_background,
    start_postgres,
    stop_processes,
    wait_server,
)

AGENT_ID = "smoke-agent"


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    procs = []
    try:
        print("==> 启动 Postgres")
        start_postgres()

        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        print("==> 启动 Server")
        procs.append(
            start_background(
                ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR],
                "/tmp/helm-smoke-server.log",
            )
        )
        wait_server()

        print("==> 启动 Agent")
        procs.append(
            start_background(
                [
                    "./target/debug/helm-agent",
                    "--agent-id", AGENT_ID,
                    "--server-addr", f"http://{GRPC_ADDR}",
                    "--token", "dev-token-change-me",
                ],
                "/tmp/helm-smoke-agent.log",
            )
        )
        time.sleep(3)

        print("==> 登录")
        token = login()

        print("==> 下发命令")
        job_id = http_json(
            "POST", "/exec",
            {"agent_id": AGENT_ID, "command": "echo", "args": ["smoke-ok"]},
            token,
        )["job_id"]
        time.sleep(2)

        print("==> 验证结果")
        job = http_json("GET", f"/jobs/{job_id}", token=token)["job"]
        assert job["status"] == "succeeded", f"unexpected status: {job['status']}"
        assert "smoke-ok" in (job["output"] or ""), "output missing smoke-ok"
        print(f"smoke OK: status={job['status']} output={job['output']!r}")
    finally:
        stop_processes(procs)


if __name__ == "__main__":
    main()
