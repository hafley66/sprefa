#!/usr/bin/env bash
# oracle.sh — the swipl reference engine as a CONTRACT.md section 2.1
# executable. This is the REFERENCE: its stdout is what every other engine is
# byte-diffed against.
#
#   oracle.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>
#
# Wraps v6/prolog/compile/scripts/dl6_oracle.pl UNMODIFIED. That door reads
# .dl6 text plus the same JSON schedule an http client posts, and prints the
# shared tick-log envelope through conformance/ticklog.pl's own
# print_ticklog/3 -- the very predicate oracle_dump.pl calls, so the reference
# log here and the corpus oracle logs come from one printer.
#
# dl6_oracle.pl uses relative ensure_loaded('../../conformance/ticklog'), so
# the run MUST cd into its directory. Both paths are made absolute first.
#
# --db is accepted and ignored: the reference engine holds its world in
# prolog, not in a database. The reason is written into the perf JSON's notes
# rather than left as a bare N/A (CONTRACT.md section 2.4).

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

# Date.now(), not performance.now(): the latter's origin is per process, so a
# reading taken in one node process minus a reading from another is not a
# duration. Epoch ms is.
start=$(node -e 'process.stdout.write(String(Date.now()))')
( cd "$ORACLE_DIR" && swipl -q -l dl6_oracle.pl -g "oracle('$PROGRAM','$SCHEDULE')" -g halt )
status=$?
end=$(node -e 'process.stdout.write(String(Date.now()))')

# Engine-reported wall for the oracle is measured by this wrapper rather than
# inside prolog, and it therefore CARRIES swipl's ~10-20ms startup floor. That
# is stated in notes rather than quietly compared against tsv2's startup-free
# number: adding a timing goal inside dl6_oracle.pl would edit a file this
# lane is fenced out of. CONTRACT.md section 6 prices the fix.
WALL_MS=$(( end - start ))
TICKS=0
if [ -n "${BENCH_TICKS_FROM:-}" ] && [ -f "${BENCH_TICKS_FROM}" ]; then
  TICKS=$(grep -c '' "${BENCH_TICKS_FROM}" 2>/dev/null || echo 0)
fi

cat > "$PERF_OUT" <<JSON
{"engine":"oracle","wall_ms":${WALL_MS},"compile_ms":"N/A","ticks":${TICKS},"statements":"N/A","db_bytes":"N/A","notes":{"compile_ms":"compile_ms: the reference engine interprets the .dl6 text, there is no separate compile phase","statements":"statements: the reference engine evaluates in prolog and issues no SQL","db_bytes":"db_bytes: the reference engine holds its world in prolog, --db is accepted and ignored","wall_ms":"wall_ms: measured by the wrapper around the whole swipl process, so it includes swipl's ~10-20ms startup floor; tsv2's wall_ms excludes node startup. Not comparable head to head -- see CONTRACT.md section 6."}}
JSON

exit $status
