#!/usr/bin/env python3
"""定时任务持久化恢复 e2e：创建 schedule → 重启 Server → 验证恢复执行。"""

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
    psql,
    psql_value,
    start_background,
    start_postgres,
    stop_processes,
    wait_server,
)

AGENT_ID = "sched-e2e-agent"
SERVER_LOG = "/tmp/helm-sched-server.log"


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    server = None
    agent = None

    def start_server() -> None:
        nonlocal server
        server = start_background(
            ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR],
            SERVER_LOG,
        )
        wait_server()

    try:
        print("==> 启动 Postgres")
        start_postgres()

        print("==> 清理旧任务与 job")
        psql("DELETE FROM tasks; DELETE FROM jobs;")

        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        print("==> 启动 Server + Agent")
        start_server()
        agent = start_background(
            [
                "./target/debug/helm-agent",
                "--agent-id", AGENT_ID,
                "--server-addr", f"http://{GRPC_ADDR}",
                "--token", "dev-token-change-me",
            ],
            "/tmp/helm-sched-agent.log",
        )
        time.sleep(3)

        print("==> 登录 + 创建定时任务")
        token = login()
        http_json(
            "POST", "/tasks/schedule",
            {
                "agent_id": AGENT_ID,
                "command": "echo",
                "args": ["sched-e2e"],
                "interval_secs": 3,
            },
            token,
        )

        time.sleep(7)
        before = int(psql_value("SELECT COUNT(*) FROM jobs WHERE command='echo';"))
        print(f"重启前 job 数: {before}")

        print("==> 重启 Server")
        server.terminate()
        server.wait(timeout=5)
        time.sleep(2)
        start_server()

        time.sleep(8)
        after = int(psql_value("SELECT COUNT(*) FROM jobs WHERE command='echo';"))
        print(f"重启后 job 数: {after}")

        with open(SERVER_LOG) as f:
            resumed = f.read().count("resumed scheduled task")
        print(f"resumed 日志条数: {resumed}")

        assert before > 0, "任务重启前未执行"
        assert after > before, f"任务重启后未恢复执行 (before={before}, after={after})"
        assert resumed >= 1, "未发现 resumed scheduled task 日志"
        print(f"✓ 定时任务持久化恢复通过: {before} -> {after} jobs, resumed={resumed}")
    finally:
        stop_processes([p for p in (server, agent) if p is not None])


if __name__ == "__main__":
    main()
