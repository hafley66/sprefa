#!/usr/bin/env bash
# oracle.sh — swipl reference adapter. --db is accepted and ignored.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_CLI="$(cd "$HERE/.." && pwd)"
ORACLE_DIR="$(cd "$BENCH_CLI/../prolog/compile/scripts" && pwd)"

PROGRAM=""; SCHEDULE=""; PERF_OUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --program)  PROGRAM="$2";  shift 2 ;;
    --schedule) SCHEDULE="$2"; shift 2 ;;
    --db)       shift 2 ;;
    --perf-out) PERF_OUT="$2"; shift 2 ;;
    *) echo "oracle.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done
if [ -z "$PROGRAM" ] || [ -z "$SCHEDULE" ] || [ -z "$PERF_OUT" ]; then
  echo "usage: oracle.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>" >&2
  exit 1
fi

PROGRAM="$(cd "$(dirname "$PROGRAM")" && pwd)/$(basename "$PROGRAM")"
SCHEDULE="$(cd "$(dirname "$SCHEDULE")" && pwd)/$(basename "$SCHEDULE")"

# Epoch milliseconds are comparable across the node wrapper and swipl process.
start=$(node -e 'process.stdout.write(String(Date.now()))')
( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl -g "oracle('$PROGRAM','$SCHEDULE')" -g halt )
status=$?
end=$(node -e 'process.stdout.write(String(Date.now()))')

# The wrapper-measured wall includes swipl startup and is recorded in notes.
WALL_MS=$(( end - start ))
TICKS=0
if [ -n "${BENCH_TICKS_FROM:-}" ] && [ -f "${BENCH_TICKS_FROM}" ]; then
  TICKS=$(grep -c '' "${BENCH_TICKS_FROM}" 2>/dev/null || echo 0)
fi

cat > "$PERF_OUT" <<JSON
{"engine":"oracle","wall_ms":${WALL_MS},"compile_ms":"N/A","ticks":${TICKS},"statements":"N/A","db_bytes":"N/A","notes":{"compile_ms":"compile_ms: the reference engine interprets the .dl6 text, there is no separate compile phase","statements":"statements: the reference engine evaluates in prolog and issues no SQL","db_bytes":"db_bytes: the reference engine holds its world in prolog, --db is accepted and ignored","wall_ms":"wall_ms: measured by the wrapper around the whole swipl process, so it includes swipl's ~10-20ms startup floor; tsv2's wall_ms excludes node startup. Not comparable head to head -- see CONTRACT.md section 6."}}
JSON

exit $status
