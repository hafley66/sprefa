#!/usr/bin/env bash
set -euo pipefail

GOLDEN_DIR="$(cd "$(dirname "$0")" && pwd)"
V6_DIR="$(cd "$GOLDEN_DIR/../../.." && pwd)"
ROOT_DIR="$(cd "$V6_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
PROGRAM="$GOLDEN_DIR/0_ghcacher_clock_golden.dl6"
SCHEDULE="$GOLDEN_DIR/1_schedule.json"
EXPECTED_TICKS="$GOLDEN_DIR/2_expected.tick.jsonl"
EXPECTED_FINAL="$GOLDEN_DIR/3_expected.final.jsonl"
EXPECTED_STATEMENTS="$GOLDEN_DIR/5_expected.statements.jsonl"
WORK_DIR="$(mktemp -d "$TSV2_DIR/.ghcacher-clock-golden.XXXXXX")"
GENERATED="$WORK_DIR/ghcacher_clock_golden.ts"
ORACLE_ACTUAL="$WORK_DIR/oracle.jsonl"
EMITTED_ACTUAL="$WORK_DIR/emitted.jsonl"
STATEMENTS_ACTUAL="$WORK_DIR/statements.jsonl"

cleanup() {
  rm -rf -- "$WORK_DIR"
}
trap cleanup EXIT

cd "$ROOT_DIR"

swipl -q -l "$V6_DIR/prolog/compile.pl" \
  -g "compile_dl6('$PROGRAM', '$GENERATED')" \
  -g halt >/dev/null

swipl -q "$GOLDEN_DIR/4_oracle.pl" -- "$PROGRAM" "$SCHEDULE" \
  >"$ORACLE_ACTUAL"

(
  cd "$TSV2_DIR"
  TSV2_STMT_RECEIPT="$STATEMENTS_ACTUAL" \
  NODE_NO_WARNINGS=1 node --experimental-transform-types \
    scripts/4_ghcacher-tick-golden.ts \
    "$GENERATED" \
    "$SCHEDULE"
) >"$EMITTED_ACTUAL"

diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$ORACLE_ACTUAL"
diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$EMITTED_ACTUAL"
diff -u "$ORACLE_ACTUAL" "$EMITTED_ACTUAL"
diff -u "$EXPECTED_STATEMENTS" "$STATEMENTS_ACTUAL"

TOTALS="$(tail -n 1 "$STATEMENTS_ACTUAL")"
printf 'GHCACHER_CLOCK_GOLDEN_HOLDS ticks=5 final=1 sql=%s\n' "$TOTALS"
