#!/usr/bin/env python3
"""helm skill 共享底座：连接配置、认证（JWT / API key）、HTTP 请求、输出。

环境变量：
  HELM_URL        Server HTTP 地址（默认 http://127.0.0.1:8080）
  HELM_TOKEN      Bearer 凭据：helm_ 前缀 API key 或 JWT（优先）
  HELM_USERNAME   登录用户名（默认 admin）
  HELM_PASSWORD   登录密码（默认 admin123，开发默认值）

无 token 时自动用用户名密码登录，JWT 缓存在系统临时目录（24h 有效期内复用）。
"""

import json
import os
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

# Windows 控制台 UTF-8 输出
for stream in (sys.stdout, sys.stderr):
    if hasattr(stream, "reconfigure"):
        stream.reconfigure(encoding="utf-8", errors="replace")

API = "/api/v1"
TOKEN_CACHE = os.path.join(tempfile.gettempdir(), "helm-skill-tokens.json")
TOKEN_TTL = 23 * 3600  # JWT 24h 过期，提前 1h 换


def base_url():
    return os.environ.get("HELM_URL", "http://127.0.0.1:8080").rstrip("/")


def _cache_load():
    try:
        with open(TOKEN_CACHE, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def _cache_save(url, token):
    cache = _cache_load()
    cache[url] = {"token": token, "ts": time.time()}
    try:
        with open(TOKEN_CACHE, "w", encoding="utf-8") as f:
            json.dump(cache, f)
    except OSError:
        pass


def _jwt_expired(token):
    """从 JWT payload 解 exp（API key 非 JWT，返回 False）。"""
    if token.startswith("helm_"):
        return False
    import base64

    try:
        payload = token.split(".")[1]
        payload += "=" * (-len(payload) % 4)
        claims = json.loads(base64.urlsafe_b64decode(payload))
        return claims.get("exp", 0) < time.time() + 60
    except (IndexError, ValueError, KeyError):
        return True


def _login():
    username = os.environ.get("HELM_USERNAME", "admin")
    password = os.environ.get("HELM_PASSWORD", "admin123")
    url = base_url()
    status, data = request("POST", "/auth/login", token=None,
                           body={"username": username, "password": password})
    if status != 200:
        die(f"登录失败（{status}）：{data}。请设置 HELM_TOKEN（推荐 API key）或 "
            f"HELM_USERNAME/HELM_PASSWORD。")
    return data["token"]


def get_token():
    """解析凭据：HELM_TOKEN > 缓存的 JWT > 用户名密码登录。"""
    env = os.environ.get("HELM_TOKEN", "").strip()
    if env:
        return env
    url = base_url()
    cached = _cache_load().get(url)
    if cached and not _jwt_expired(cached["token"]):
        return cached["token"]
    token = _login()
    _cache_save(url, token)
    return token


def request(method, path, token=None, body=None, query=None):
    """发 HTTP 请求。path 不含 /api/v1 前缀。返回 (status, 解析后的 JSON)。"""
    url = base_url() + API + path
    if query:
        url += "?" + urllib.parse.urlencode(query)
    req = urllib.request.Request(url, method=method)
    if token:
        req.add_header("Authorization", f"Bearer {token}")
    data = None
    if body is not None:
        req.add_header("Content-Type", "application/json")
        data = json.dumps(body, ensure_ascii=False).encode("utf-8")
    try:
        with urllib.request.urlopen(req, data, timeout=60) as resp:
            raw = resp.read()
            return resp.status, json.loads(raw) if raw else {}
    except urllib.error.HTTPError as e:
        raw = e.read()
        try:
            return e.code, json.loads(raw)
        except ValueError:
            return e.code, {"error": {"code": "raw", "message": raw.decode(errors="replace")}}
    except urllib.error.URLError as e:
        die(f"连接失败 {url}：{e.reason}（Server 未启动？检查 HELM_URL）")


def api(method, path, body=None, query=None, args=None):
    """带认证的 API 调用；401 且持有可刷新凭据时自动重登一次。"""
    token = getattr(api, "_token", None) or get_token()
    api._token = token
    status, data = request(method, path, token, body, query)
    if status == 401 and not os.environ.get("HELM_TOKEN"):
        # 缓存 JWT 失效 → 重新登录重试一次
        token = _login()
        api._token = token
        _cache_save(base_url(), token)
        status, data = request(method, path, token, body, query)
    return status, data


def call(method, path, body=None, query=None):
    """api() + 非 2xx 即退出。"""
    status, data = api(method, path, body, query)
    if status >= 400:
        err = data.get("error", data)
        die(f"{method} {API}{path} -> {status}: {json.dumps(err, ensure_ascii=False)}")
    return data


def output(data, raw=False):
    if raw:
        print(data)
    else:
        print(json.dumps(data, ensure_ascii=False, indent=2))


def die(msg, code=1):
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(code)


def add_common_args(p):
    p.add_argument("--url", help="Server HTTP 地址（默认 HELM_URL 或 http://127.0.0.1:8080）")
    p.add_argument("--json", action="store_true", help="输出原始 JSON（默认已是 JSON）")


def apply_common_args(args):
    if getattr(args, "url", None):
        os.environ["HELM_URL"] = args.url


def parse_list(s):
    """逗号分隔字符串 -> 列表。"""
    return [x for x in (s or "").split(",") if x]


def split_dashdash(argv=None):
    """按 `--` 把 argv 分成两段：返回 (前段列表, 命令列表)。
    命令段原样保留（含 - 开头的参数），交由远端解释。"""
    argv = list(sys.argv[1:] if argv is None else argv)
    if "--" in argv:
        i = argv.index("--")
        return argv[:i], argv[i + 1:]
    return argv, []
