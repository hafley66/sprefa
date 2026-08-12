#!/usr/bin/env bash
# dd-diet-rust-rust bench-cli adapter: compile a .dl6 program through the
# dd_plan emitter option, then drive dd-runner's pure-RAM kernel arm. stdout is
# the tick log and nothing else (contract clause 2.1: stdout is what gets
# byte-diffed). The compiler's own stdout and the dd-runner result are routed
# around it.
#
# Exit codes: 0 ran, 2 named refusal, 1 error.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_CLI="$(cd "$HERE/.." && pwd)"
ROOT="$(cd "$BENCH_CLI/../.." && pwd)"

PROGRAM=""; SCHEDULE=""; DB=":memory:"; PERF_OUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --program)  PROGRAM="$2";  shift 2 ;;
    --schedule) SCHEDULE="$2"; shift 2 ;;
    --db)       DB="$2";       shift 2 ;;
    --perf-out) PERF_OUT="$2"; shift 2 ;;
    *) echo "dd-diet-rust-rust.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done
if [ -z "$PROGRAM" ] || [ -z "$SCHEDULE" ] || [ -z "$PERF_OUT" ]; then
  echo "usage: dd-diet-rust-rust.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>" >&2
  exit 1
fi

PROGRAM="$(cd "$(dirname "$PROGRAM")" && pwd)/$(basename "$PROGRAM")"
SCHEDULE="$(cd "$(dirname "$SCHEDULE")" && pwd)/$(basename "$SCHEDULE")"

NAME="$(basename "$PROGRAM" .dl6)"
mkdir -p "$BENCH_CLI/out"
PLAN="$BENCH_CLI/out/$NAME.dd.json"
COMPILE_LOG="$BENCH_CLI/out/$NAME.dd.compile.log"
RUNNER="$ROOT/v6/dd-runner/target/release/dd-runner"

# ── phase 1: compile through the dd emitter option ──────────────────────────
# The compiler prints "wrote ..." and a COMPILE-TRACE line to stdout; both are
# routed into a log so the compile cannot corrupt the tick log on stdout.
compile_start=$(node -e 'process.stdout.write(String(Date.now()))')
swipl -q -l "$ROOT/v6/prolog/compile.pl" \
         -l "$ROOT/v6/prolog/compile/6_emit_dd_plan.pl" \
      -g "compile_dl6('$PROGRAM','$PLAN',[emitter(emit_dd_plan:emit_program),schedule('$SCHEDULE')])" \
      -g halt > "$COMPILE_LOG" 2>&1
compile_status=$?
compile_end=$(node -e 'process.stdout.write(String(Date.now()))')
COMPILE_MS=$(( compile_end - compile_start ))

if [ "$compile_status" -ne 0 ]; then
  cat "$COMPILE_LOG" >&2
  # Named refusals are recorded separately from errors.
  if grep -qE 'unsupported_construct|not_stratified|column_mismatch|bind_mismatch|bind_and_rule_head|probe_mismatch|query_mismatch|refused_host_decl|template_mismatch|unmapped_feature' "$COMPILE_LOG"; then
    exit 2
  fi
  exit 1
fi

# ── phase 2: run dd-runner's kernel arm ────────────────────────────────────
# dd-runner's stdout IS the tick log; stream it to stdout so the harness can
# byte-diff it, and count the ticks off the same stream.
TICKLOG="$BENCH_CLI/out/$NAME.dd.ticklog"
run_start=$(node -e 'process.stdout.write(String(Date.now()))')
"$RUNNER" "$PLAN" --dd-diet-rust-rust > "$TICKLOG" 2> "$BENCH_CLI/out/$NAME.dd.err"
run_status=$?
run_end=$(node -e 'process.stdout.write(String(Date.now()))')
WALL_MS=$(( run_end - run_start ))

cat "$TICKLOG"
[ "$run_status" -ne 0 ] && cat "$BENCH_CLI/out/$NAME.dd.err" >&2

# Final state is not produced: dd-runner emits the tick log, not a final-state
# line, so the third check (CONTRACT 2.7) is unmet and no file is faked.
TICKS=$(grep -c '' "$TICKLOG" 2>/dev/null || echo 0)
cat > "$PERF_OUT" <<JSON
{"engine":"dd-diet-rust-rust","wall_ms":${WALL_MS},"compile_ms":${COMPILE_MS},"ticks":${TICKS},"statements":"N/A","db_bytes":"N/A","notes":{"statements":"statements: this arm is pure-RAM and issues no SQL","db_bytes":"db_bytes: this arm holds its world in memory; --db is accepted and ignored","final_state":"final_state: dd-runner emits the tick log but no final-state line, so the 2.7 final-state hash is not produced (CONTRACT.md Priced and not taken)"}}
JSON

exit "$run_status"
