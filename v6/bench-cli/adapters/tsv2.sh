#!/usr/bin/env bash
# tsv2.sh — compile a .dl6 program, then run the compiled module.
# Exit codes: 0 ran, 2 named refusal, 1 error.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_CLI="$(cd "$HERE/.." && pwd)"
COMPILE_DL6="$BENCH_CLI/../prolog/compile/scripts/compile_dl6.sh"

PROGRAM=""; SCHEDULE=""; DB=":memory:"; PERF_OUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --program)  PROGRAM="$2";  shift 2 ;;
    --schedule) SCHEDULE="$2"; shift 2 ;;
    --db)       DB="$2";       shift 2 ;;
    --perf-out) PERF_OUT="$2"; shift 2 ;;
    *) echo "tsv2.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done
if [ -z "$PROGRAM" ] || [ -z "$SCHEDULE" ] || [ -z "$PERF_OUT" ]; then
  echo "usage: tsv2.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>" >&2
  exit 1
fi

NAME="$(basename "$PROGRAM" .dl6)"
MODULE="$BENCH_CLI/out/$NAME.ts"
COMPILE_LOG="$BENCH_CLI/out/$NAME.compile.log"
mkdir -p "$BENCH_CLI/out"

# ── phase 1: compile ────────────────────────────────────────────────────────
# Recompile each run so generated output cannot be stale. Epoch milliseconds
# are used for the cross-process compile duration.
compile_start=$(node -e 'process.stdout.write(String(Date.now()))')
bash "$COMPILE_DL6" "$PROGRAM" "$MODULE" > "$COMPILE_LOG" 2>&1
compile_status=$?
compile_end=$(node -e 'process.stdout.write(String(Date.now()))')
COMPILE_MS=$(( compile_end - compile_start ))

if [ "$compile_status" -ne 0 ]; then
  cat "$COMPILE_LOG" >&2
  # Named refusals are recorded separately from errors.
  if grep -qE 'unsupported_construct|not_stratified|column_mismatch|bind_mismatch|bind_and_rule_head|probe_mismatch|query_mismatch|refused_host_decl|template_mismatch|unmapped_feature|keyed_level_head|latest_in_level_rule|pre_in_level_rule|finalize_in_level_rule|log_on_level_headed_rel|keep_on_non_log_rel|partial_head|join_column_type_mismatch|edge_head_column_type_mismatch|trigger_arg_not_var' "$COMPILE_LOG"; then
    exit 2
  fi
  exit 1
fi

# ── phase 2: run ────────────────────────────────────────────────────────────
cd "$BENCH_CLI"
node --experimental-transform-types \
  "$HERE/tsv2_run.ts" \
  --program "$MODULE" \
  --schedule "$SCHEDULE" \
  --db "$DB" \
  --perf-out "$PERF_OUT" \
  --compile-ms "$COMPILE_MS"
