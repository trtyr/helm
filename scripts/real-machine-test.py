#!/usr/bin/env python3
"""真机全量测试：对指定 agent 跑 Phase 5-8 全部能力，输出逐项结果。

用法：
  HELM_TEST_AGENT=win-real HELM_TEST_PLATFORM=win python3 scripts/real-machine-test.py
  HELM_TEST_AGENT=linux-real HELM_TEST_PLATFORM=linux python3 scripts/real-machine-test.py

环境变量：
  HELM_TEST_AGENT     目标 agent_id（必填）
  HELM_TEST_PLATFORM  win | linux（决定命令语法，默认 linux）
  HELM_E2E_HTTP_ADDR  Server HTTP 地址（默认 127.0.0.1:18080）
"""

import asyncio
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from e2e_helpers import HTTP_ADDR, http_json, login

AGENT = os.environ.get("HELM_TEST_AGENT", "")
PLATFORM = os.environ.get("HELM_TEST_PLATFORM", "linux")

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(f"{'✓' if ok else '✗'} {name}" + (f" — {detail}" if detail else ""))


def sh(cmd: str):
    """按平台返回 [shell, args]。"""
    if PLATFORM == "win":
        return ("cmd", ["/c", cmd])
    return ("sh", ["-c", cmd])


def test_exec(token):
    shell, args = sh("echo REAL_MACHINE_MARKER_$((100+1)) 2>/dev/null || echo REAL_MACHINE_MARKER_101")
    job = http_json("POST", "/exec", {"agent_id": AGENT, "command": shell, "args": args}, token)
    jid = job["job_id"]
    time.sleep(2)
    j = http_json("GET", f"/jobs/{jid}", token=token)["job"]
    check("命令执行", j["status"] == "succeeded" and "REAL_MACHINE_MARKER" in (j["output"] or ""),
          f"status={j['status']} exit={j['exit_code']}")
    return jid


def test_files(token):
    import tempfile
    tmp = tempfile.mkdtemp()
    local = os.path.join(tmp, "up.txt")
    content = "real-machine-file-content\n"
    with open(local, "w") as f:
        f.write(content)
    remote = "/tmp/helm-real-up.txt" if PLATFORM == "linux" else "C:\\Users\\16933\\helm-real-up.txt"
    up = http_json("POST", "/files/upload", {"agent_id": AGENT, "local_path": local, "remote_path": remote}, token)
    check("文件上传", up.get("checksum_ok") is True, f"checksum_ok={up.get('checksum_ok')}")

    down_local = os.path.join(tmp, "down.txt")
    down = http_json("POST", "/files/download", {"agent_id": AGENT, "remote_path": remote, "local_path": down_local}, token)
    ok = down.get("checksum_ok") is True and open(down_local).read() == content
    check("文件下载", ok)

    ls_path = "/tmp" if PLATFORM == "linux" else "C:\\Users\\16933"
    entries = http_json("POST", "/files/list", {"agent_id": AGENT, "path": ls_path}, token).get("entries", [])
    check("列目录", len(entries) > 0, f"{len(entries)} 项")


async def test_terminal(token):
    import websockets
    shell, _ = sh("echo")
    uri = f"ws://{HTTP_ADDR}/api/v1/agents/{AGENT}/terminal?token={token}"
    marker = "PTY_REAL_MARKER"
    try:
        async with websockets.connect(uri) as ws:
            await asyncio.sleep(2.0)  # 等 shell prompt
            if PLATFORM == "win":
                await ws.send(f"echo {marker}\r\n")
            else:
                await ws.send(f"echo {marker}\r")
            out = b""
            deadline = time.time() + 10
            while time.time() < deadline:
                try:
                    m = await asyncio.wait_for(ws.recv(), timeout=1.0)
                except asyncio.TimeoutError:
                    continue
                out += m if isinstance(m, bytes) else m.encode()
                if marker.encode() in out:
                    break
            check("交互终端", marker.encode() in out, f"收到 {len(out)} 字节回显")
    except Exception as e:
        check("交互终端", False, f"异常: {e}")


def test_service(token):
    shell, args = sh("while true; do echo SVC_TICK; sleep 1; done")
    svc = http_json("POST", "/services", {
        "agent_id": AGENT, "name": "real-svc", "command": shell, "args": args, "restart_policy": "no",
    }, token)["service"]
    sid = svc["id"]
    http_json("POST", f"/services/{sid}/start", token=token)
    time.sleep(3)
    svc = next(s for s in http_json("GET", "/services", token=token)["services"] if s["id"] == sid)
    log = http_json("GET", f"/services/{sid}/logs", token=token).get("log", "")
    ok = svc["status"] == "running" and "SVC_TICK" in log
    check("服务管理", ok, f"status={svc['status']} log_len={len(log)}")
    http_json("POST", f"/services/{sid}/stop", token=token)
    return sid


def test_process_net(token):
    procs = http_json("POST", "/processes/list", {"agent_id": AGENT}, token).get("processes", [])
    check("进程列表", len(procs) > 0, f"{len(procs)} 个进程")

    net = http_json("POST", "/net/info", {"agent_id": AGENT}, token)
    check("网络信息", bool(net.get("hostname")), f"hostname={net.get('hostname')}")

    # kill 一个不存在的 pid → ok=false
    kill = http_json("POST", "/processes/kill", {"agent_id": AGENT, "pid": 999999}, token)
    check("进程 kill", kill.get("ok") is False, "不存在 pid 返回 ok=false")


def test_online(token):
    agents = http_json("GET", "/agents", token=token)["agents"]
    a = next((x for x in agents if x["id"] == AGENT), None)
    check("在线状态", bool(a and a.get("online")), f"online={a and a.get('online')}")


def test_listener(token):
    ln = http_json("POST", "/listeners", {"name": "real-ln", "addr": "127.0.0.1:15930"}, token)["listener"]
    lid = ln["id"]
    http_json("POST", f"/listeners/{lid}/start", token=token)
    ls = http_json("GET", "/listeners", token=token)["listeners"]
    started = any(x["id"] == lid and x["status"] == "running" for x in ls)
    check("监听器启停", started, "动态启动 running")
    http_json("POST", f"/listeners/{lid}/stop", token=token)
    http_json("DELETE", f"/listeners/{lid}", token=token)


def test_audit(token):
    audit = http_json("GET", "/audit?limit=50", token=token).get("audit", [])
    check("审计落库", len(audit) > 0, f"{len(audit)} 条审计记录")


def test_metrics(token):
    agents = http_json("GET", "/agents", token=token)["agents"]
    host_id = next(a["host_id"] for a in agents if a["id"] == AGENT)
    metrics = http_json("GET", f"/metrics?host_id={host_id}&limit=100", token=token).get("metrics", [])
    names = {m["name"] for m in metrics}
    check("监控指标", len(metrics) > 0, f"{len(metrics)} 条指标，含 {sorted(names)[:4]}")


async def test_stream(token):
    import websockets
    # job 输出流
    shell, args = sh("sleep 2; echo JOB_STREAM_REAL")
    job = http_json("POST", "/exec", {"agent_id": AGENT, "command": shell, "args": args}, token)
    jid = job["job_id"]
    uri = f"ws://{HTTP_ADDR}/api/v1/jobs/{jid}/stream?token={token}"
    try:
        async with websockets.connect(uri) as ws:
            out = b""
            deadline = time.time() + 10
            while time.time() < deadline:
                try:
                    m = await asyncio.wait_for(ws.recv(), timeout=1.0)
                except asyncio.TimeoutError:
                    continue
                out += m if isinstance(m, bytes) else m.encode()
                if b"JOB_STREAM_REAL" in out:
                    break
            check("job 实时流", b"JOB_STREAM_REAL" in out, f"{len(out)} 字节")
    except Exception as e:
        check("job 实时流", False, f"异常: {e}")


def main():
    if not AGENT:
        sys.exit("请设置 HELM_TEST_AGENT")
    token = login()
    print(f"==> 真机测试 agent={AGENT} platform={PLATFORM} http={HTTP_ADDR}\n")

    test_online(token)
    test_exec(token)
    test_files(token)
    test_process_net(token)
    test_service(token)
    test_listener(token)
    test_audit(token)
    test_metrics(token)
    print()
    asyncio.run(test_terminal(token))
    asyncio.run(test_stream(token))

    print("\n==> 结果汇总 ==")
    ok = sum(1 for _, o, _ in results if o)
    for name, o, detail in results:
        print(f"  {'✓' if o else '✗'} {name}" + (f" — {detail}" if detail else ""))
    print(f"\n通过 {ok}/{len(results)}")
    sys.exit(0 if ok == len(results) else 1)


if __name__ == "__main__":
    main()
