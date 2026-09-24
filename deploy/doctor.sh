#!/usr/bin/env bash
# helm doctor · P007 T1 —— 部署依赖自检（宿主层 + 容器层）
#
# 缘起：sg 首次真实部署暴露的坑全在这——zig 缺失、.cargo-musl 脚本不在、rustup
# targets 没装、registry cache/src 为空（容器内 cargo build 卡联网）、mingw 没有、
# 端口被占、.env 占位值没改。这些本该一条命令报出来，而不是等到 agent 生成失败。
#
# 用法：
#   ./deploy/doctor.sh            # 宿主层 + 容器层（容器在跑时）
#   ./deploy/doctor.sh --host-only
#
# 说明：
# - 端口检查针对 prod 默认（80/443/50051）；用 HELM_BIND_* 覆盖过端口的，自行对号。
# - 80/443 被占不一定是错（可能是本部署自己的 Caddy 或既有反代复用），记 warn。
# - exit 0 = 无 FAIL（warn 不影响）；有 FAIL 时 exit 1。

set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="$ROOT/deploy/prod/.env"
SERVER_CONTAINER="helm-prod-server"

HOST_ONLY=0
[ "${1:-}" = "--host-only" ] && HOST_ONLY=1

PASS=0; FAIL=0; WARN=0
ok()   { echo "  [ok]   $*"; PASS=$((PASS+1)); }
bad()  { echo "  [FAIL] $*"; FAIL=$((FAIL+1)); }
warn() { echo "  [warn] $*"; WARN=$((WARN+1)); }

# --- 宿主层 ---

echo "== helm doctor · 宿主层 =="

if command -v docker >/dev/null 2>&1; then
  ok "docker: $(docker --version 2>/dev/null | head -1)"
else
  bad "docker 未安装（https://docs.docker.com/get-docker/）"
fi

if docker compose version >/dev/null 2>&1; then
  ok "docker compose: $(docker compose version 2>/dev/null | head -1)"
else
  bad "docker compose 子命令不可用（需 Docker Compose v2）"
fi

port_listen() { (echo >/dev/tcp/127.0.0.1/"$1") >/dev/null 2>&1; }

for p in 80 443; do
  if port_listen "$p"; then
    warn "端口 $p 已被占用——若是本部署的 Caddy 或既有反代复用则正常；否则需换端口（HELM_BIND_*）或复用已有反代，见 deploy/prod/README"
  else
    ok "端口 $p 空闲"
  fi
done

if port_listen 50051; then
  warn "端口 50051 已被占用——gRPC 对外端口，需确认是本部署自身或其他服务"
else
  ok "端口 50051 空闲"
fi

if [ -f "$ENV_FILE" ]; then
  ok "deploy/prod/.env 存在"
  for key in POSTGRES_PASSWORD HELM_SERVER_TOKEN HELM_JWT_SECRET; do
    v="$(grep -E "^${key}=" "$ENV_FILE" 2>/dev/null | head -1 | cut -d= -f2-)"
    if [ -z "$v" ]; then
      bad "$key 为空（必改 3 处之一）"
    elif [[ "$v" == CHANGE_ME* ]]; then
      bad "$key 仍是占位值（openssl rand -hex 32 生成后填入）"
    else
      ok "$key 已设置"
    fi
  done
else
  bad "deploy/prod/.env 不存在——先 cp deploy/prod/env.example deploy/prod/.env 并改必改 3 处"
fi

disk_free_mb="$(df -m "$ROOT" 2>/dev/null | awk 'NR==2{print $4}')"
if [ -n "${disk_free_mb:-}" ] && [ "$disk_free_mb" -lt 5120 ]; then
  warn "磁盘剩余 ${disk_free_mb}MB（构建 + 镜像建议 ≥5GB）"
elif [ -n "${disk_free_mb:-}" ]; then
  ok "磁盘剩余 ${disk_free_mb}MB"
fi

# --- 容器层（server 容器在跑时） ---

if [ "$HOST_ONLY" -eq 1 ]; then
  echo "（--host-only：跳过容器层）"
elif docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^${SERVER_CONTAINER}$"; then
  echo
  echo "== 容器层（$SERVER_CONTAINER）== agent-gen 现场出包前提"
  dexec() { docker exec "$SERVER_CONTAINER" sh -c "$1" >/dev/null 2>&1; }

  if dexec 'test -x /opt/zig/zig'; then ok "zig 就位（/opt/zig/zig）"; else bad "zig 缺失——镜像构建于工具链修复前？重新 docker compose build"; fi

  if dexec 'test -x /workspace/.cargo-musl/bin/x86_64-linux-musl-gcc'; then ok "musl 包装脚本就位（.cargo-musl/bin）"; else bad "x86_64-linux-musl-gcc 包装脚本缺失——agent 生成 linux 目标会失败（EN-66 路径）"; fi

  if dexec 'rustup target list --installed | grep -q "^x86_64-unknown-linux-musl$"'; then ok "rustup target: x86_64-unknown-linux-musl"; else bad "rustup 缺 x86_64-unknown-linux-musl target"; fi

  if dexec 'rustup target list --installed | grep -q "^x86_64-pc-windows-gnu$"'; then ok "rustup target: x86_64-pc-windows-gnu"; else bad "rustup 缺 x86_64-pc-windows-gnu target"; fi

  if dexec 'find /usr/local/cargo/registry/cache -name "*.crate" -print -quit | grep -q .'; then ok "registry cache 含 .crate（离线编译可用）"; else bad "registry cache 无 .crate——容器内 cargo build 会联网下载（EN-2 复发），重建镜像"; fi

  if dexec 'find /usr/local/cargo/registry/src -mindepth 2 -maxdepth 2 -print -quit | grep -q .'; then ok "registry src 含依赖源码"; else bad "registry src 为空——容器内编译必失败（EN-2 复发）"; fi

  if dexec 'command -v x86_64-w64-mingw32-gcc'; then ok "mingw 就位（windows-gnu 链接器）"; else bad "x86_64-w64-mingw32-gcc 不在 PATH——windows agent 生成会失败（P2）"; fi

  if dexec 'curl -fsS -o /dev/null http://127.0.0.1:8080/healthz'; then ok "server /healthz 响应"; else warn "容器内 /healthz 无响应——server 可能还在启动"; fi
else
  echo
  warn "容器 $SERVER_CONTAINER 未运行——跳过容器层（docker compose up -d --build 后再跑一次检查 agent-gen 前提）"
fi

# --- 汇总 ---

echo
echo "汇总：$PASS ok / $WARN warn / $FAIL fail"
if [ "$FAIL" -gt 0 ]; then
  echo "有 FAIL 项——按上面提示处理后重跑。"
  exit 1
fi
echo "宿主侧检查通过。$( [ "$HOST_ONLY" -eq 0 ] && docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^${SERVER_CONTAINER}$" && echo '容器层亦通过。' )"
exit 0
