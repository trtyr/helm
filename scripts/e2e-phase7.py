#!/usr/bin/env python3
"""Phase 7 e2e：mTLS 双向认证（Agent token 换证书 → gRPC mTLS 握手）。"""

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
    wait_for,
    wait_server,
)

AGENT_ID = "phase7-mtls-agent"
CERT_DIR = "/tmp/helm-phase7-cert"


def agent_online(token: str) -> bool:
    agents = http_json("GET", "/agents", token=token)["agents"]
    return any(a["id"] == AGENT_ID and a.get("online") for a in agents)


def main() -> None:
    os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
    procs = []
    try:
        print("==> 启动 Postgres")
        start_postgres()
        print("==> 构建")
        subprocess.run(["cargo", "build", "-p", "helm-server", "-p", "helm-agent"], check=True)

        print("==> 启动 Server（--mtls）")
        procs.append(
            start_background(
                ["./target/debug/helm-server", "--http-addr", HTTP_ADDR, "--grpc-addr", GRPC_ADDR, "--mtls"],
                "/tmp/helm-phase7-server.log",
            )
        )
        wait_server()

        print("==> 启动 Agent（cert-dir 换证书 → mTLS）")
        subprocess.run(["rm", "-rf", CERT_DIR], check=False)
        procs.append(
            start_background(
                [
                    "./target/debug/helm-agent",
                    "--agent-id", AGENT_ID,
                    "--server-addr", f"http://{GRPC_ADDR}",
                    "--server-http-addr", f"http://{HTTP_ADDR}",
                    "--token", "dev-token-change-me",
                    "--cert-dir", CERT_DIR,
                ],
                "/tmp/helm-phase7-agent.log",
            )
        )

        token = login()
        assert wait_for(lambda: agent_online(token), timeout=15), "agent 未上线（mTLS 握手失败？）"
        print("✓ mTLS 握手通过（agent 换证书后经双向认证注册上线）")

        assert os.path.exists(os.path.join(CERT_DIR, "cert.pem")), "缺 cert.pem"
        assert os.path.exists(os.path.join(CERT_DIR, "key.pem")), "缺 key.pem"
        assert os.path.exists(os.path.join(CERT_DIR, "ca.pem")), "缺 ca.pem"
        print("✓ 证书缓存落盘（cert.pem / key.pem / ca.pem）")

        print("==> 审计日志")
        audit = http_json("GET", "/audit", token=token)["audit"]
        assert any(a["action"] == "login" for a in audit), "缺少 login 审计"
        http_json("POST", "/exec", {"agent_id": AGENT_ID, "command": "echo", "args": ["audit-marker"]}, token)
        audit2 = http_json("GET", "/audit", token=token)["audit"]
        assert any(a["action"] == "exec" for a in audit2), "缺少 exec 审计"
        print("✓ 审计日志通过（login + exec 落库可查）")

        print("==> 监控 + 告警")
        alerts = http_json("GET", "/alerts", token=token)["alerts"]
        assert isinstance(alerts, list), "alerts 应为数组"
        print(f"✓ 告警端点可用（当前 {len(alerts)} 条告警）")
    finally:
        stop_processes(procs)


if __name__ == "__main__":
    main()
