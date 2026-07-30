#!/usr/bin/env bash
set -euo pipefail

LAB_DIR="$(cd "$(dirname "$0")" && pwd)"
V6_DIR="$(cd "$LAB_DIR/../../.." && pwd)"
ROOT_DIR="$(cd "$V6_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
PROGRAM="$LAB_DIR/0_ghcacher_clock_golden.dl6"
SCHEDULE="$LAB_DIR/1_schedule.json"
EXPECTED_TICKS="$LAB_DIR/2_expected.tick.jsonl"
EXPECTED_FINAL="$LAB_DIR/3_expected.final.jsonl"
WORK_DIR="$(mktemp -d "$TSV2_DIR/.ghcacher-clock-golden.XXXXXX")"
GENERATED="$WORK_DIR/ghcacher_clock_golden.ts"
ORACLE_ACTUAL="$WORK_DIR/oracle.jsonl"
EMITTED_ACTUAL="$WORK_DIR/emitted.jsonl"

cleanup() {
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

cd "$ROOT_DIR"

swipl -q -l "$V6_DIR/prolog/compile/compile.pl" \
  -g "compile_dl6('$PROGRAM', '$GENERATED')" \
  -g halt >/dev/null

swipl -q "$LAB_DIR/4_oracle.pl" -- "$PROGRAM" "$SCHEDULE" \
  >"$ORACLE_ACTUAL"

(
  cd "$TSV2_DIR"
  NODE_NO_WARNINGS=1 node --experimental-transform-types \
    scripts/4_ghcacher-tick-golden.ts \
    "$GENERATED" \
    "$SCHEDULE"
) >"$EMITTED_ACTUAL"

diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$ORACLE_ACTUAL"
diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$EMITTED_ACTUAL"
diff -u "$ORACLE_ACTUAL" "$EMITTED_ACTUAL"

printf 'GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1\n'
