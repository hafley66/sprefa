#!/usr/bin/env bash
# tsv2.sh — the tsv2 engine as a CONTRACT.md section 2.1 executable.
#
#   tsv2.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>
#
# Two phases, and the split is the contract's:
#   1. compile  .dl6 text -> TS module, via the UNMODIFIED
#      v6/prolog/compile/scripts/compile_dl6.sh. Timed, reported as
#      compile_ms, and NOT part of wall_ms.
#   2. run      the module over the schedule, via adapters/tsv2_run.ts.
#
# Exit codes per CONTRACT.md section 2.1: 0 ran, 2 named refusal, 1 error.
# A compile that fails with one of the named refusal functors is a REFUSAL,
# not a failure -- the same classification bop.ts does on a 400 body, and the
# functor list is kept in step with it deliberately (bop.ts
# NAMED_REASON_FUNCTORS, itself a mirror of bop_check.pl's
# named_reason_functor/1; three copies now, all documented as mirrors because
# each sees the reason in a different form -- a live term, an HTTP body, a
# swipl stderr line).

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
# Recompiled every run rather than cached: a stale compiled module is the
# gen_staleness_gate class of defect this repo has been bitten by four times
# (door-handwritten.ts), and the compile is cheap next to the run.
# Date.now(), not performance.now(): performance.now()'s origin is PER
# PROCESS, so subtracting one process's reading from another's is meaningless
# (it produced a negative compile_ms before this was caught). Epoch ms is
# comparable across processes, and 1ms resolution is ample for a compile that
# runs in the hundreds of ms.
compile_start=$(node -e 'process.stdout.write(String(Date.now()))')
bash "$COMPILE_DL6" "$PROGRAM" "$MODULE" > "$COMPILE_LOG" 2>&1
compile_status=$?
compile_end=$(node -e 'process.stdout.write(String(Date.now()))')
COMPILE_MS=$(( compile_end - compile_start ))

if [ "$compile_status" -ne 0 ]; then
  cat "$COMPILE_LOG" >&2
  # A named refusal is a designed answer, not a bug: exit 2 so the harness
  # records REFUSED with the reason instead of ERROR.
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
