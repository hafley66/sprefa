#!/usr/bin/env bash
# dd-diet-rust-sqlite bench-cli adapter: refuses under contract clause 2.1 (the arm takes a dd_plan JSON with embedded schedule; no .dl6-text-to-dd_plan door exists, 6_emit_dd_plan.pl:33 / compile.pl:328).

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"

PROGRAM=""; SCHEDULE=""; DB=":memory:"; PERF_OUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --program)  PROGRAM="$2";  shift 2 ;;
    --schedule) SCHEDULE="$2"; shift 2 ;;
    --db)       DB="$2";       shift 2 ;;
    --perf-out) PERF_OUT="$2"; shift 2 ;;
    *) echo "dd-diet-rust-sqlite.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done
if [ -z "$PROGRAM" ] || [ -z "$SCHEDULE" ] || [ -z "$PERF_OUT" ]; then
  echo "usage: dd-diet-rust-sqlite.sh --program <file.dl6> --schedule <s.json> --db <path> --perf-out <p.json>" >&2
  exit 1
fi

REASON="contract clause 2.1: --program is a .dl6 TEXT file and the schedule is external to it, but the dd-diet-rust-sqlite arm takes a dd_plan JSON whose initial + schedule the emitter embeds (6_emit_dd_plan.pl:33); no .dl6-text-to-dd_plan door exists (compile.pl:328 emits emit_ts only). See CONTRACT.md Priced and not taken"

cat > "$PERF_OUT" <<JSON
{"engine":"dd-diet-rust-sqlite","wall_ms":"N/A","compile_ms":"N/A","ticks":"N/A","statements":"N/A","db_bytes":"N/A","notes":{"refusal":"$REASON"}}
JSON

echo "dd-diet-rust-sqlite.sh: refused: $REASON" >&2
exit 2
