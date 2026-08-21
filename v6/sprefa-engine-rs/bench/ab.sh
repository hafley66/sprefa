#!/usr/bin/env bash
# @comment-ok: the bench's usage contract, the one doc site for its arguments.
# ab.sh <per|shared> <runs> <ENV=VALUE ...> -- interleave A and B runs of the
# sf_join fold, A being the current default and B the environment given, so a
# machine that drifts drifts through both arms.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/.." && pwd)"
MODE="$1"
RUNS="$2"
shift 2
OUT="${TMPDIR:-/tmp}/ab_${MODE}"
now() { python3 -c 'import time;print(int(time.time()*1000))'; }
for index in $(seq 1 "$RUNS"); do
  START=$(now)
  "$ENGINE/target/release/emit_rust_harness" "$HERE/sf_join_${MODE}.rs" "$HERE/sf_join_big.json" \
    >"$OUT.a.jsonl" 2>/dev/null
  A=$(($(now) - START))
  START=$(now)
  env "$@" "$ENGINE/target/release/emit_rust_harness" "$HERE/sf_join_${MODE}.rs" "$HERE/sf_join_big.json" \
    >"$OUT.b.jsonl" 2>/dev/null
  B=$(($(now) - START))
  cmp -s "$OUT.a.jsonl" "$OUT.b.jsonl" && SAME=identical || SAME=DIFFERENT
  printf '%s run %s a_ms=%s b_ms=%s %s\n' "$MODE" "$index" "$A" "$B" "$SAME"
done
