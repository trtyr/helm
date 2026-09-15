"""E2E 测试共享工具（纯标准库，无第三方依赖）。

提供 HTTP 调用、Postgres 启动/查询、进程后台启动/清理。
供 e2e-smoke.py / e2e-scheduler.py / e2e-phase5.py 复用。
"""

import json
import os
import subprocess
import sys
import time
import urllib.request

HTTP_ADDR = os.environ.get("HELM_E2E_HTTP_ADDR", "127.0.0.1:8080")
GRPC_ADDR = os.environ.get("HELM_E2E_GRPC_ADDR", "127.0.0.1:50051")
BASE = f"http://{HTTP_ADDR}/api/v1"


def http_json(method: str, path: str, body=None, token: str | None = None) -> dict:
    """发起 JSON HTTP 请求，返回解析后的 dict。

    超时给 60s：debug 构建下 bcrypt 登录校验（cost 12）约需 30–40s，
    10s 会在高负载机器上偶发超时。
    """
    url = f"{BASE}{path}"
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(url, data=data, method=method)
    req.add_header("Content-Type", "application/json")
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(req, timeout=60) as resp:
        return json.loads(resp.read().decode())


def login(username: str = "admin", password: str = "admin123") -> str:
    """登录控制台，返回 Bearer token。"""
    return http_json("POST", "/auth/login", {"username": username, "password": password})["token"]


def start_postgres(timeout: int = 30) -> None:
    """启动 Postgres 并等待 healthy。"""
    subprocess.run(["docker", "compose", "up", "-d", "postgres"], check=True)
    for _ in range(timeout):
        s = subprocess.run(
            ["docker", "inspect", "-f", "{{.State.Health.Status}}", "helm-postgres"],
            capture_output=True,
            text=True,
        ).stdout.strip()
        if s == "healthy":
            return
        time.sleep(1)
    sys.exit("Postgres 未在 {}s 内就绪".format(timeout))


def psql(sql: str) -> str:
    """执行一条 SQL，返回 stdout。"""
    out = subprocess.run(
        ["docker", "compose", "exec", "-T", "postgres", "psql", "-U", "helm", "-d", "helm", "-c", sql],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(f"psql 失败: {out.stderr}")
    return out.stdout


def psql_value(sql: str) -> str:
    """执行 SQL 并返回单个标量值（-tAc）。"""
    out = subprocess.run(
        ["docker", "compose", "exec", "-T", "postgres", "psql", "-U", "helm", "-d", "helm", "-tAc", sql],
        capture_output=True,
        text=True,
    )
    if out.returncode != 0:
        sys.exit(f"psql 失败: {out.stderr}")
    return out.stdout.strip()


def start_background(args: list[str], logfile: str):
    """后台启动进程，stdout/stderr 写日志文件，返回 Popen。"""
    log = open(logfile, "w")
    return subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT)


def free_addr() -> str:
    """找一个空闲端口，返回 '127.0.0.1:{port}'。"""
    import socket

    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return f"127.0.0.1:{s.getsockname()[1]}"


def wait_for(predicate, timeout: float = 10.0, interval: float = 0.5) -> bool:
    """轮询等待 predicate() 返回真值，超时返回 False。"""
    deadline = time.time() + timeout
    while time.time() < deadline:
        if predicate():
            return True
        time.sleep(interval)
    return False


def wait_server(timeout: float = 30.0) -> None:
    """轮询 /healthz 直到 HTTP 服务就绪，超时退出。"""
    url = f"http://{HTTP_ADDR}/healthz"

    def _ok() -> bool:
        try:
            with urllib.request.urlopen(url, timeout=2) as resp:
                return resp.status == 200
        except Exception:
            return False

    if not wait_for(_ok, timeout=timeout):
        sys.exit("Server 未在 {}s 内就绪".format(timeout))


def stop_processes(procs) -> None:
    """终止后台进程列表（best-effort）。"""
    for p in procs:
        if p.poll() is None:
            p.terminate()
    for p in procs:
        try:
            p.wait(timeout=5)
        except subprocess.TimeoutExpired:
            p.kill()
