#!/usr/bin/env bash
# @comment-ok: the bench's usage contract, the one doc site for its arguments.
# file-db.sh <per|shared> <runs> [ENV=VALUE ...] -- fold sf_join against a
# FILE-backed seam (DL_DB_URL), a fresh database per run, so the pragmas and the
# per-tick transaction that are no-ops on `:memory:` can be measured at all.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ENGINE="$(cd "$HERE/.." && pwd)"
MODE="$1"
RUNS="$2"
shift 2
DB="${TMPDIR:-/tmp}/sf_join_file.db"
OUT="${TMPDIR:-/tmp}/sf_join_file"
now() { python3 -c 'import time;print(int(time.time()*1000))'; }
for index in $(seq 1 "$RUNS"); do
  rm -f "$DB" "$DB-journal" "$DB-wal" "$DB-shm"
  START=$(now)
  env DL_DB_URL="$DB" "$@" "$ENGINE/target/release/emit_rust_harness" \
    "$HERE/sf_join_${MODE}.rs" "$HERE/sf_join_big.json" >"$OUT.$index.jsonl" 2>"$OUT.$index.summary"
  printf 'file %s run %s wall_ms=%s size_mb=%s\n' "$MODE" "$index" "$(($(now) - START))" \
    "$(du -m "$DB" | cut -f1)"
done
