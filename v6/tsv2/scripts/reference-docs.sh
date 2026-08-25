#!/usr/bin/env bash
# reference-docs.sh: boots tsv2, loads reference-docs-rail.dl6, writes v6/prolog/compile/CONSTRUCT-REFERENCE.md.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
TSV2="$(cd "$SCRIPT_DIR/.." && pwd)"
V6="$(cd "$TSV2/.." && pwd)"
ROOT="$(cd "$V6/.." && pwd)"

. "$V6/tools/run-capped.sh"
cap_self "${REFERENCE_DOCS_BUDGET_S:-600}" reference_docs "$@"

PROGRAM="$V6/dl/fixtures/reference-docs-rail.dl6"
OUT="$V6/prolog/compile/CONSTRUCT-REFERENCE.md"
SERVE="$TSV2/serve/main.ts"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/reference-docs.XXXXXX")"
PORT="${REFERENCE_DOCS_PORT:-17721}"
BASE="http://127.0.0.1:$PORT"
SERVER_PID=""

export DL_COMMENT_NODE="${DL_COMMENT_NODE:-$SCRIPT_DIR/comment_node.py}"
export DL_POLICY_MARKERS="${DL_POLICY_MARKERS:-$SCRIPT_DIR/comment_policy_markers.py}"
if [ -z "${DL_EXTRACT_BIN:-}" ]; then
  DL_EXTRACT_BIN="$ROOT/v6/sprefa-extract/target/release/extract"
  export DL_EXTRACT_BIN
fi

say() { printf '%s\n' "$*"; }
die() { say "FAIL  $*"; exit 1; }

stop_server() {
  if [ -n "$SERVER_PID" ]; then
    kill -9 "$SERVER_PID" 2>/dev/null
    wait "$SERVER_PID" 2>/dev/null
    SERVER_PID=""
  fi
}
trap stop_server EXIT

[ -f "$PROGRAM" ] || die "program is missing: $PROGRAM"
command -v swipl >/dev/null || die "swipl is not on PATH"
command -v jq >/dev/null || die "jq is not on PATH"
[ -x "$DL_EXTRACT_BIN" ] || die "extractor is not executable: $DL_EXTRACT_BIN"

for source_path in v6/prolog/0_dot_expand/registry.pl; do
  ( cd "$ROOT" && git ls-files --error-unmatch -- "$source_path" ) >/dev/null 2>&1 \
    || die "watched source is not tracked by git: $source_path"
done

(
  cd "$ROOT"
  TSV2_DB="file:$WORK/reference-docs.sqlite" TSV2_PORT="$PORT" \
    TSV2_WATCH_ROOT="$ROOT" TSV2_WATCH_COALESCE_MS=60 \
    node --experimental-transform-types "$SERVE"
) >"$WORK/server.log" 2>&1 &
SERVER_PID=$!

for ((attempt=1; attempt<=100; attempt++)); do
  capped_curl "${REFERENCE_DOCS_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null && break
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.2
done
capped_curl "${REFERENCE_DOCS_HTTP_BUDGET_S:-30}" -s -o /dev/null "$BASE/ticks" 2>/dev/null || die "server did not become ready"

status="$(capped_curl "${REFERENCE_DOCS_LOAD_BUDGET_S:-900}" -s -o "$WORK/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
[ "$status" = 200 ] || die "program load returned $status: $(cat "$WORK/load.json")"

RELS="source write_receipt"
fetch_all() {
  local target="$1" rel first=1
  : >"$target"
  printf '{' >>"$target"
  for rel in $RELS; do
    [ "$first" = 1 ] || printf ',' >>"$target"
    first=0
    printf '"%s":' "$rel" >>"$target"
    capped_curl "${REFERENCE_DOCS_HTTP_BUDGET_S:-30}" -s "$BASE/idb/$rel" >>"$target" || return 1
  done
  printf '}' >>"$target"
}

settled=0
for ((attempt=1; attempt<=240; attempt++)); do
  fetch_all "$WORK/rows.json" || die "read failed; server log: $(tail -30 "$WORK/server.log")"
  if jq -e '(.source.rows | length) == 1 and (.write_receipt.rows | length) == 1 and .write_receipt.rows[0][1] == "written"' "$WORK/rows.json" >/dev/null 2>&1; then
    settled=1
    break
  fi
  kill -0 "$SERVER_PID" 2>/dev/null || die "server died: $(tail -30 "$WORK/server.log")"
  sleep 0.5
done
[ "$settled" = 1 ] || die "rels did not settle in 120s; server log: $(tail -30 "$WORK/server.log")"

say "REFERENCE DOCS WROTE $OUT"
awk 'BEGIN { c = 0 } /^\| `/ { c++ } END { print "  rows=" c " lines=" NR }' "$OUT"

stop_server
say "REFERENCE DOCS HOLDS"
