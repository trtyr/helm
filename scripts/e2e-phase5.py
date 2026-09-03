#!/usr/bin/env python3
"""Phase 5 e2e：监听器启停 + hosts 在线状态 + agent 注销/卸载。"""

import os
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import (
    GRPC_ADDR,
    HTTP_ADDR,
    free_addr,
    http_json,
    login,
    psql,
    start_background,
    start_postgres,
    stop_processes,
    wait_for,
    wait_server,
)

UNINSTALL_BIN = "/tmp/helm-agent-phase5-e2e"


def agent_args(agent_id: str) -> list[str]:
    return [
        "./target/debug/helm-agent",
        "--agent-id", agent_id,
        "--server-addr", f"http://{GRPC_ADDR}",
        "--token", "dev-token-change-me",
    ]


def find_agent(token: str, agent_id: str):
    agents = http_json("GET", "/agents", token=token)["agents"]
    return next((a for a in agents if a["id"] == agent_id), None)


def agent_online(token: str, agent_id: str) -> bool:
    a = find_agent(token, agent_id)
    return bool(a and a.get("online"))


def host_online(token: str, host_id: str) -> bool:
    hosts = http_json("GET", "/hosts?limit=1000", token=token)["hosts"]
    h = next((x for x in hosts if x["id"] == host_id), None)
    return bool(h and h.get("online"))


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    server = None
    procs: list = []

    def start_server() -> None:
        nonlocal server
        server = start_background(
            ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR],
            "/tmp/helm-phase5-server.log",
        )
        wait_server()

    try:
        print("==> 启动 Postgres")
        start_postgres()
        print("==> 清理 listeners")
        psql("DELETE FROM listeners;")
        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        print("==> 启动 Server")
        start_server()
        procs.append(server)
        token = login()

        # ── 1. 监听器：create / list / start / stop ──────────────────────
        print("==> [监听器] 默认 seed 检查")
        listeners = http_json("GET", "/listeners", token=token)["listeners"]
        default = next((l for l in listeners if l["name"] == "default"), None)
        assert default is not None and default["running"], "默认监听器未 seed / 未 running"

        addr = free_addr()
        print(f"==> [监听器] 创建 second @ {addr}")
        created = http_json("POST", "/listeners", {"name": "phase5-second", "addr": addr}, token)["listener"]
        assert not created["running"], "新建监听器应为 stopped"
        lid = created["id"]

        http_json("POST", f"/listeners/{lid}/start", token=token)
        listeners = http_json("GET", "/listeners", token=token)["listeners"]
        second = next(l for l in listeners if l["id"] == lid)
        assert second["running"], "second 监听器 start 后未 running"

        http_json("POST", f"/listeners/{lid}/stop", token=token)
        listeners = http_json("GET", "/listeners", token=token)["listeners"]
        second = next(l for l in listeners if l["id"] == lid)
        assert not second["running"], "second 监听器 stop 后未 stopped"
        print("✓ 监听器 create/list/start/stop 通过")

        # ── 2. 在线状态：hosts online 翻转 ───────────────────────────────
        print("==> [在线状态] 启动 agent")
        online_agent = start_background(agent_args("phase5-online-agent"), "/tmp/helm-phase5-online-agent.log")
        procs.append(online_agent)
        assert wait_for(lambda: agent_online(token, "phase5-online-agent"), timeout=15), "agent 未上线"
        host_id = find_agent(token, "phase5-online-agent")["host_id"]
        assert host_online(token, host_id), "host 未显示 online"

        print("==> [在线状态] 断开 agent")
        online_agent.terminate()
        online_agent.wait(timeout=5)
        assert wait_for(lambda: not host_online(token, host_id), timeout=15), "host 未翻转为 offline"
        print("✓ hosts online 翻转通过")

        # ── 3. 注销：DELETE /agents/{id} ────────────────────────────────
        print("==> [注销] 启动 agent 后 DELETE")
        dereg_agent = start_background(agent_args("phase5-dereg-agent"), "/tmp/helm-phase5-dereg-agent.log")
        procs.append(dereg_agent)
        assert wait_for(lambda: find_agent(token, "phase5-dereg-agent") is not None, timeout=15), "agent 未注册"
        http_json("DELETE", "/agents/phase5-dereg-agent", token=token)
        assert find_agent(token, "phase5-dereg-agent") is None, "agent 未注销"
        dereg_agent.terminate()
        dereg_agent.wait(timeout=5)
        print("✓ DELETE 注销通过")

        # ── 4. 卸载：POST /agents/{id}/uninstall ─────────────────────────
        print("==> [卸载] 复制二进制到 /tmp 并启动")
        shutil.copy("target/debug/helm-agent", UNINSTALL_BIN)
        os.chmod(UNINSTALL_BIN, 0o755)
        uninstall_agent = start_background(
            [
                UNINSTALL_BIN,
                "--agent-id", "phase5-uninstall-agent",
                "--server-addr", f"http://{GRPC_ADDR}",
                "--token", "dev-token-change-me",
            ],
            "/tmp/helm-phase5-uninstall-agent.log",
        )
        procs.append(uninstall_agent)
        assert wait_for(lambda: agent_online(token, "phase5-uninstall-agent"), timeout=15), "卸载 agent 未上线"

        http_json("POST", "/agents/phase5-uninstall-agent/uninstall", {"remove_binary": True}, token)
        time.sleep(2)
        assert not os.path.exists(UNINSTALL_BIN), "卸载后二进制仍存在"
        assert find_agent(token, "phase5-uninstall-agent") is None, "卸载后 agent 未注销"
        print("✓ uninstall 卸载通过（二进制删除 + 注销）")

        print("✓✓ Phase 5 e2e 全部通过")
    finally:
        stop_processes(procs)
        if os.path.exists(UNINSTALL_BIN):
            os.remove(UNINSTALL_BIN)


if __name__ == "__main__":
    main()
