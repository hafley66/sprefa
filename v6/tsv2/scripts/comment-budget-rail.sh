#!/usr/bin/env bash
# The dl6 comment-budget rail, live: grade the staged diff, exit 2 on findings.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TSV2_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
V6_DIR="$(cd "$TSV2_DIR/.." && pwd)"
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null)" || REPO_ROOT="$(cd "$V6_DIR/.." && pwd)"
PROGRAM="${COMMENT_RAIL_PROGRAM:-$TSV2_DIR/goldens/comment_rail_golden/0_comment_rail_golden.dl6}"
IDLE_MS="${COMMENT_RAIL_IDLE_MS:-700}"
MAX_RUN=2

export DL_COMMENT_BUDGET_FEED="${DL_COMMENT_BUDGET_FEED:-$SCRIPT_DIR/comment-budget-feed.sh}"
export DL_COMMENT_NODE="${DL_COMMENT_NODE:-$SCRIPT_DIR/comment_node.py}"
export DL_EXTRACT_BIN="${DL_EXTRACT_BIN:-$V6_DIR/sprefa-extract/target/release/extract}"

if [ ! -x "$DL_EXTRACT_BIN" ]; then
  echo "comment-budget rail: extractor missing at $DL_EXTRACT_BIN" >&2
  echo "  build: (cd $V6_DIR/sprefa-extract && cargo build --release --features cli --bin extract)" >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/comment-budget-rail.XXXXXX")"
SERVER_PID=""
TICKS_PID=""
cleanup() {
  [ -n "$TICKS_PID" ] && kill -9 "$TICKS_PID" 2>/dev/null
  [ -n "$SERVER_PID" ] && kill -9 "$SERVER_PID" 2>/dev/null
  wait 2>/dev/null
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

PORT="${COMMENT_RAIL_PORT:-0}"
if [ "$PORT" = 0 ]; then
  PORT="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
fi
BASE="http://127.0.0.1:$PORT"

START_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

(
  cd "$REPO_ROOT"
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
  echo "comment-budget rail: server did not start" >&2
  tail -20 "$WORK_DIR/server.log" >&2
  exit 1
fi

curl -sN "$BASE/ticks" >"$WORK_DIR/ticks.sse" 2>/dev/null &
TICKS_PID=$!

status="$(curl -s -o "$WORK_DIR/load.json" -w '%{http_code}' -X POST --data-binary @"$PROGRAM" "$BASE/program")"
if [ "$status" != 200 ]; then
  echo "comment-budget rail: program load returned $status" >&2
  cat "$WORK_DIR/load.json" >&2
  exit 1
fi

# The grading key: the staged tree's own hash, so re-grading one index is one
# witness and a restage is a new one.
INDEX_DIGEST="$(git write-tree 2>/dev/null || printf 'index-unknown')"
printf '{"batch":[{"rel":"max_run","sign":"add","row":[%d]},{"rel":"staged_probe","sign":"add","row":["%s"]}]}\n' \
  "$MAX_RUN" "$INDEX_DIGEST" >"$WORK_DIR/arrivals.json"
status="$(curl -s -o "$WORK_DIR/arrivals.out" -w '%{http_code}' -X POST --data-binary @"$WORK_DIR/arrivals.json" "$BASE/arrivals")"
if [ "$status" != 200 ]; then
  echo "comment-budget rail: arrivals returned $status" >&2
  cat "$WORK_DIR/arrivals.out" >&2
  exit 1
fi

# Quiescence is the same rule `bop run` uses: no tick event for IDLE_MS. Host
# rounds are what keep it resetting, so this is "every extractor has answered".
last_size=-1
idle_slept=0
step_ms=50
deadline=$((SECONDS + 60))
while [ "$SECONDS" -lt "$deadline" ]; do
  size="$(wc -c <"$WORK_DIR/ticks.sse" 2>/dev/null | tr -d ' ')"
  if [ "$size" = "$last_size" ]; then
    idle_slept=$((idle_slept + step_ms))
    [ "$idle_slept" -ge "$IDLE_MS" ] && break
  else
    idle_slept=0
    last_size="$size"
  fi
  sleep 0.05
done

curl -s "$BASE/idb/violation_run" >"$WORK_DIR/violation_run.json"
curl -s "$BASE/idb/graded_file" >"$WORK_DIR/graded_file.json"
END_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

python3 "$SCRIPT_DIR/comment_budget_report.py" "$WORK_DIR/violation_run.json" "$MAX_RUN"
VERDICT=$?

if [ -n "${COMMENT_RAIL_STATS:-}" ]; then
  printf 'comment-budget rail (dl6): %d ms, graded files %s\n' \
    "$((END_MS - START_MS))" \
    "$(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["rows"]))' "$WORK_DIR/graded_file.json" 2>/dev/null || echo '?')" >&2
fi

exit "$VERDICT"
