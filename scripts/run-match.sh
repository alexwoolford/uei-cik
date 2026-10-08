#!/usr/bin/env bash
# Oneshot: exact UEI-to-CIK match into work sqlite.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="${UEI_CIK_BIN:-$ROOT/bin/uei-cik}"
STATE="${UEI_CIK_STATE:-/var/lib/uei-cik}"
DB="${UEI_CIK_SQLITE:-$STATE/uei-cik.sqlite}"
CACHE="${UEI_CIK_CACHE:-$STATE/http-cache}"
LOCK="${UEI_CIK_LOCK:-$STATE/.match.lock}"

test -x "$BIN" || {
  echo "missing $BIN — build with: cargo build --release" >&2
  exit 1
}

mkdir -p "$STATE" "$CACHE"

if command -v flock >/dev/null 2>&1; then
  exec 9>"$LOCK"
  if ! flock -n 9; then
    echo "match already running (lock $LOCK)" >&2
    exit 1
  fi
else
  if ! mkdir "$LOCK.d" 2>/dev/null; then
    echo "match already running (lock $LOCK.d)" >&2
    exit 1
  fi
  trap 'rmdir "$LOCK.d" 2>/dev/null || true' EXIT
fi

echo "== uei-cik match =="
echo "bin=$BIN db=$DB cache=$CACHE"
"$BIN" --db "$DB" match --cache-dir "$CACHE"
