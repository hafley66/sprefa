#!/usr/bin/env bash
# devlog.sh -- served rail for v6/dl/fixtures/devlog.dl6.
#
# The DL6 program owns fact selection, joins, markdown construction, ordinal
# assignment, and group_concat document assembly. This script owns process
# lifecycle, the HTTP load/read bridge, and no document text construction.
# The final write is the write_devlog host declared in the program.

# Whole-script budget; override with DEVLOG_BUDGET_S.
#
# The cap is on the WHOLE script: the cost is a backgrounded node server, the
# ledger hosts it spawns and two poll loops, none of which this script waits
# on directly. `cap_self` (v6/tools/run-capped.sh) puts all of it in one
# process group and SIGKILLs the group on the budget, exit 124.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"

. "$V6/tools/run-capped.sh"
cap_self "${DEVLOG_BUDGET_S:-600}" devlog "$@"

PROGRAM="$V6/dl/fixtures/devlog.dl6"
SERVE="$TSV2/serve/main.ts"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/devlog.XXXXXX")"
PORT="${DEVLOG_PORT:-17731}"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

say() { printf '%s\n' "$*"; }
die() { say "FAIL  $*"; exit 1; }

stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null || true
    wait "$SERVER_PID" 2>/dev/null || true
    SERVER_PID=""
  fi
}
trap stop_server EXIT

[ -f "$PROGRAM" ] || die "program is missing: $PROGRAM"
[ -f "$SERVE" ] || die "served runner is missing: $SERVE"
command -v curl >/dev/null || die "curl is not on PATH"
command -v swipl >/dev/null || die "swipl is not on PATH"

(
  cd "$ROOT"
  TSV2_DB="file:$WORK/devlog.sqlite" TSV2_PORT="$PORT" \
    TSV2_WATCH_ROOT="$ROOT" TSV2_WATCH_COALESCE_MS=60 \
    node --experimental-transform-types "$SERVE"
) >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

attempt=1
while [ "$attempt" -le 120 ]; do
  if capped_curl "${DEVLOG_HTTP_BUDGET_S:-10}" -s -o /dev/null "$BASE/ticks" 2>/dev/null; then
    break
  fi
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.2
  attempt=$((attempt + 1))
done
capped_curl "${DEVLOG_HTTP_BUDGET_S:-10}" -s -o /dev/null "$BASE/ticks" 2>/dev/null || die "server did not become ready"

status="$(capped_curl "${DEVLOG_LOAD_BUDGET_S:-900}" -s -o "$WORK/load.json" -w '%{http_code}' -X POST \
  --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || die "program load returned $status: $(cat "$WORK/load.json")"

attempt=1
while [ "$attempt" -le 240 ]; do
  if [ -f "$ROOT/DEVLOG.md" ] && capped_curl "${DEVLOG_HTTP_BUDGET_S:-10}" -s "$BASE/idb/write_receipt" | \
    grep -q 'DEVLOG.md'; then
    break
  fi
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.5
  attempt=$((attempt + 1))
done

# The write host fires on every PARTIAL document as ledger responses stream
# in (each partial is its own content address). Wait for quiescence so the
# LAST write is the one we grade: DEVLOG.md digest stable across 1s.
# NOTE: /ticks is a STREAMING endpoint (200, holds the connection open);
# a bare curl read of it blocks forever. Grade the written file instead.
prev_digest=""
attempt=1
while [ "$attempt" -le 60 ]; do
  now_digest="$(shasum "$ROOT/DEVLOG.md" 2>/dev/null | cut -d' ' -f1)"
  if [ -n "$prev_digest" ] && [ "$now_digest" = "$prev_digest" ]; then break; fi
  prev_digest="$now_digest"
  sleep 1
  attempt=$((attempt + 1))
done

[ -f "$ROOT/DEVLOG.md" ] || die "DEVLOG.md was not written: $(tail -30 "$WORK/server.log")"
capped_curl "${DEVLOG_HTTP_BUDGET_S:-10}" -s "$BASE/idb/write_receipt" | grep -q 'DEVLOG.md' \
  || die "write_receipt did not arrive: $(tail -30 "$WORK/server.log")"

lines="$(wc -l < "$ROOT/DEVLOG.md" | tr -d ' ')"
say "DEVLOG WROTE $ROOT/DEVLOG.md lines=$lines"
say "DEVLOG HOLDS"
