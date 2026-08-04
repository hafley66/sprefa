#!/usr/bin/env bash
# Hermetic gate for the dl6 comment-budget rail. Receipts: README.md.
set -euo pipefail

GOLDEN_DIR="$(cd "$(dirname "$0")" && pwd)"
V6_DIR="$(cd "$GOLDEN_DIR/../../.." && pwd)"
ROOT_DIR="$(cd "$V6_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
PROGRAM="$GOLDEN_DIR/0_comment_rail_golden.dl6"
SCHEDULE="$GOLDEN_DIR/1_schedule.json"
SCALE_SCHEDULE="$GOLDEN_DIR/5_schedule.scale.json"
EXPECTED_TICKS="$GOLDEN_DIR/2_expected.tick.jsonl"
EXPECTED_FINAL="$GOLDEN_DIR/3_expected.final.jsonl"
WORK_DIR="$(mktemp -d "$TSV2_DIR/.comment-rail-golden.XXXXXX")"
GENERATED="$WORK_DIR/comment_rail_golden.ts"
ORACLE_ACTUAL="$WORK_DIR/oracle.jsonl"
EMITTED_ACTUAL="$WORK_DIR/emitted.jsonl"
SCALE_ACTUAL="$WORK_DIR/emitted.scale.jsonl"

cleanup() { rm -rf -- "$WORK_DIR"; }
trap cleanup EXIT

cd "$ROOT_DIR"
START_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"

# Schedules are generated, so a hand edit to either JSON fails here rather than
# drifting away from the generator that documents the fixture.
python3 "$GOLDEN_DIR/5_gen_schedules.py" "$WORK_DIR/regen.json" "$WORK_DIR/regen.scale.json"
diff -u "$SCHEDULE" "$WORK_DIR/regen.json"
diff -u "$SCALE_SCHEDULE" "$WORK_DIR/regen.scale.json"

swipl -q -l "$V6_DIR/prolog/compile.pl" \
  -g "compile_dl6('$PROGRAM', '$GENERATED')" \
  -g halt >/dev/null

swipl -q "$GOLDEN_DIR/4_oracle.pl" -- "$PROGRAM" "$SCHEDULE" >"$ORACLE_ACTUAL"

run_emitted() {
  (
    cd "$TSV2_DIR"
    NODE_NO_WARNINGS=1 node --experimental-transform-types \
      scripts/4_ghcacher-tick-golden.ts "$GENERATED" "$1"
  )
}

run_emitted "$SCHEDULE" >"$EMITTED_ACTUAL"
run_emitted "$SCALE_SCHEDULE" >"$SCALE_ACTUAL"

diff -u <(cat "$EXPECTED_TICKS"; cat "$EXPECTED_FINAL") "$ORACLE_ACTUAL"
diff -u <(cat "$EXPECTED_TICKS"; cat "$EXPECTED_FINAL") "$EMITTED_ACTUAL"
diff -u "$ORACLE_ACTUAL" "$EMITTED_ACTUAL"

python3 "$GOLDEN_DIR/7_assert.py" "$EMITTED_ACTUAL" "$SCALE_ACTUAL" "$GENERATED"

END_MS="$(python3 -c 'import time; print(int(time.time()*1000))')"
printf 'COMMENT_RAIL_GOLDEN_HOLDS ticks=5 final=1 wall_ms=%d\n' "$((END_MS - START_MS))"
