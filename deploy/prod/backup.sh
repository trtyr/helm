#!/usr/bin/env bash
# helm 数据库备份（在 deploy/prod 目录执行）
#
#   ./backup.sh                 # 备份到 ./data/backups/helm-<UTC时间>.sql.gz
#   ./backup.sh --keep 30       # 保留最近 30 份（默认 14）
#   ./backup.sh --restore FILE  # 从备份恢复（会覆盖现有数据，先停应用）
#
# 备份为 `pg_dump --clean --if-exists` 明文转储：恢复时先 DROP 同名对象再重建，
# 因此可以灌回**已有内容的库**（否则会撞 "already exists" 而只恢复一半）。
#
# 建议用 cron/systemd timer 每天跑一次；备份文件与 .env 同级，注意权限与异地副本。
#
# 兼容性：脚本只用 bash 3.2 也有的特性——不用 `mapfile`（bash 4+），
# 变量后紧跟中文时一律写 `${VAR}`（bash 3.2 会把多字节字符并进变量名，实测 2026-09-21）。
set -euo pipefail

cd "$(dirname "$0")"

KEEP=14
ACTION="backup"
RESTORE_FILE=""

while [[ $# -gt 0 ]]; do
	case "$1" in
	--keep)
		KEEP="$2"
		shift 2
		;;
	--restore)
		ACTION="restore"
		RESTORE_FILE="$2"
		shift 2
		;;
	*)
		echo "未知参数: $1" >&2
		exit 2
		;;
	esac
done

# shellcheck disable=SC1091
set -a && source .env && set +a
USER_DB="${POSTGRES_USER:-helm}"
NAME_DB="${POSTGRES_DB:-helm}"

if [[ "$ACTION" == "backup" ]]; then
	OUT_DIR="./data/backups"
	mkdir -p "$OUT_DIR"
	STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
	OUT="$OUT_DIR/helm-$STAMP.sql.gz"

	echo "→ 备份 $NAME_DB 到 $OUT"
	docker compose exec -T postgres pg_dump --clean --if-exists -U "$USER_DB" -d "$NAME_DB" | gzip >"$OUT"
	echo "✓ 完成：$(du -h "$OUT" | cut -f1)"

	# 轮转：只保留最近 KEEP 份
	# 注意：不用 `mapfile`（bash 4+ 内置，macOS 自带 bash 3.2 没有——实测 2026-09-21）
	OLD="$(ls -1t "$OUT_DIR"/helm-*.sql.gz 2>/dev/null | tail -n +$((KEEP + 1)) || true)"
	if [ -n "$OLD" ]; then
		printf '%s\n' "$OLD" | xargs rm -f
		echo "✓ 清理旧备份 $(printf '%s\n' "$OLD" | wc -l | tr -d ' ') 份（保留最近 ${KEEP}）"
	fi
else
	[[ -f "$RESTORE_FILE" ]] || {
		echo "备份文件不存在: $RESTORE_FILE" >&2
		exit 1
	}
	echo "⚠ 将用 $RESTORE_FILE 覆盖数据库 ${NAME_DB}（现库内容会丢）"
	echo "  先停应用：docker compose stop server"
	read -r -p "确认继续？(yes/NO) " ans
	[[ "$ans" == "yes" ]] || {
		echo "已取消"
		exit 1
	}
	gunzip -c "$RESTORE_FILE" | docker compose exec -T postgres psql -U "$USER_DB" -d "$NAME_DB"
	echo "✓ 恢复完成；重开应用：docker compose start server"
fi
