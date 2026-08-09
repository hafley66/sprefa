#!/usr/bin/env bash
# catalog-audit-rail.sh: serve the §8 audit, post the probe, assert zero rows.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TSV2_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
V6_DIR="$(cd "$TSV2_DIR/.." && pwd)"
FIXTURES="$V6_DIR/dl/fixtures"
COMPILE="$V6_DIR/prolog/compile/scripts/compile_dl6.sh"
PROGRAM="$FIXTURES/catalog-audit-rail.dl6"
IDLE_MS="${CATALOG_AUDIT_IDLE_MS:-600}"

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/catalog-audit-rail.XXXXXX")"
SERVER_PID=""
cleanup() {
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

if ! "$COMPILE" "$PROGRAM" "$WORK_DIR/audit.ts" >"$WORK_DIR/compile.log" 2>&1; then
  echo "catalog-audit rail: compile failed" >&2
  tail -20 "$WORK_DIR/compile.log" >&2
  exit 1
fi

PORT="${CATALOG_AUDIT_PORT:-0}"
if [ "$PORT" = 0 ]; then
  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
fi
BASE="http://127.0.0.1:$PORT"

(
  cd "$V6_DIR"
  TSV2_DB=":memory:" TSV2_PORT="$PORT" NODE_NO_WARNINGS=1 \
    node --experimental-transform-types "$TSV2_DIR/serve/main.ts"
) >"$WORK_DIR/server.log" 2>&1 &
SERVER_PID=$!

ready=0
for _ in $(seq 1 200); do
  if curl -s -o /dev/null --max-time 1 "$BASE/stats" 2>/dev/null; then ready=1; break; fi
  kill -0 "$SERVER_PID" 2>/dev/null || break
  sleep 0.05
done
if [ "$ready" != 1 ]; then
  echo "catalog-audit rail: server did not start" >&2
  tail -20 "$WORK_DIR/server.log" >&2
  exit 1
fi

status="$(curl -s -o "$WORK_DIR/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
if [ "$status" != 200 ]; then
  echo "catalog-audit rail: program load returned $status" >&2
  cat "$WORK_DIR/load.json" >&2
  exit 1
fi

printf '{"batch":[{"rel":"audit_probe","sign":"add","row":[1]}]}\n' >"$WORK_DIR/events.json"
status="$(curl -s -o /dev/null -w '%{http_code}' -X POST --data-binary @"$WORK_DIR/events.json" "$BASE/edb/events")"
if [ "$status" != 200 ]; then
  echo "catalog-audit rail: probe arrival returned $status" >&2
  exit 1
fi

sleep 1
UNDECODED="$(curl -s "$BASE/idb/undecoded_interned_column")"
ORPHAN="$(curl -s "$BASE/idb/orphan_view")"

echo "catalog-audit rail (dl6): undecoded=$UNDECODED orphan=$ORPHAN"
if [ "$UNDECODED" != '{"rows":[]}' ] || [ "$ORPHAN" != '{"rows":[]}' ]; then
  echo "catalog-audit rail: audit rows are not zero" >&2
  exit 1
fi
echo "CATALOG AUDIT HOLDS: zero plane/table findings across served __rel"
