#!/usr/bin/env bash
# @comment-ok: the bench's usage contract, the one doc site for its arguments.
# profile.sh <per|shared> [runs] -- fold the 54k-row sf_join schedule through
# the release harness with the summary table armed, printing wall ms per run and
# the last run's table. The emitted programs are the two frontier strategies of
# v6/tsv2/tests/shared_frontier/sf_join.dl6.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/.." && pwd)"
MODE="${1:-per}"
RUNS="${2:-3}"
OUT="${TMPDIR:-/tmp}/sf_join_${MODE}"
for index in $(seq 1 "$RUNS"); do
  START=$(python3 -c 'import time;print(int(time.time()*1000))')
  DL_TRACE_SUMMARY=1 "$ENGINE/target/release/emit_rust_harness" \
    "$HERE/sf_join_${MODE}.rs" "$HERE/sf_join_big.json" \
    >"$OUT.$index.jsonl" 2>"$OUT.$index.summary"
  END=$(python3 -c 'import time;print(int(time.time()*1000))')
  printf '%s run %s wall_ms=%s\n' "$MODE" "$index" "$((END - START))"
done
cat "$OUT.$RUNS.summary"
