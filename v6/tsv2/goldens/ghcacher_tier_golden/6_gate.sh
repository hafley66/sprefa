#!/usr/bin/env bash
set -euo pipefail

GOLDEN_DIR="$(cd "$(dirname "$0")" && pwd)"
V6_DIR="$(cd "$GOLDEN_DIR/../../.." && pwd)"
ROOT_DIR="$(cd "$V6_DIR/.." && pwd)"
TSV2_DIR="$V6_DIR/tsv2"
PROGRAM="$GOLDEN_DIR/0_ghcacher_tier_golden.dl6"
SCHEDULE="$GOLDEN_DIR/1_schedule.json"
EXPECTED_TICKS="$GOLDEN_DIR/2_expected.tick.jsonl"
EXPECTED_FINAL="$GOLDEN_DIR/3_expected.final.jsonl"
WORK_DIR="$(mktemp -d "$TSV2_DIR/.ghcacher-tier-golden.XXXXXX")"
GENERATED="$WORK_DIR/ghcacher_tier_golden.ts"
ORACLE_ACTUAL="$WORK_DIR/oracle.jsonl"
EMITTED_ACTUAL="$WORK_DIR/emitted.jsonl"

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
  NODE_NO_WARNINGS=1 node --experimental-transform-types \
    scripts/4_ghcacher-tick-golden.ts \
    "$GENERATED" \
    "$SCHEDULE"
) >"$EMITTED_ACTUAL"

diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$ORACLE_ACTUAL"
diff -u <(sed -n '1,999p' "$EXPECTED_TICKS"; sed -n '1p' "$EXPECTED_FINAL") "$EMITTED_ACTUAL"
diff -u "$ORACLE_ACTUAL" "$EMITTED_ACTUAL"

# COUNT leg: for each expected tick line, count the poll rows naming the hot
# and cold repos. The cold band has period 30 (due on buckets 120 and 150, the
# 4th and 5th clock ticks); the hot band has period 1 (due on every clock tick).
# A non-due bucket contributes ZERO poll rows for that repo.
count_poll_rows() {
  local line="$1" repo="$2" n
  n="$(perl -ne 'print "$1\n" if /"poll":\{"add":\[(.*?)\]],"del"/' <<<"$line" \
    | grep -o "\"$repo\"" | wc -l | tr -d ' ')" || n=0
  printf '%s\n' "$n"
}

cold_due=(0 0 0 1 1 0)
hot_due=(1 1 1 1 1 0)
cold_total=0
hot_total=0
mapfile -t TICK_LINES <"$EXPECTED_TICKS"
for i in "${!TICK_LINES[@]}"; do
  line="${TICK_LINES[$i]}"
  cold_have="$(count_poll_rows "$line" org/cold)"
  hot_have="$(count_poll_rows "$line" org/hot)"
  if [ "$cold_have" -ne "${cold_due[$i]}" ]; then
    echo "tier golden: cold poll count on tick $((i + 1)) is $cold_have, want ${cold_due[$i]}" >&2
    exit 1
  fi
  if [ "$hot_have" -ne "${hot_due[$i]}" ]; then
    echo "tier golden: hot poll count on tick $((i + 1)) is $hot_have, want ${hot_due[$i]}" >&2
    exit 1
  fi
  cold_total=$((cold_total + cold_have))
  hot_total=$((hot_total + hot_have))
done

# The aliased slug-list aggregation and its points budget, from the measured
# 5.3 value at bucket 120: batch 0 holds 'org/cold org/hot' for one call.
grep -q '"batch_query":{"add":\[\[120,0,"org/cold org/hot"\]\]' "$EXPECTED_TICKS" \
  || { echo "tier golden: slug list at bucket 120 is not 'org/cold org/hot'" >&2; exit 1; }
grep -q '"points_budget":{"add":\[\[120,1\]\]' "$EXPECTED_TICKS" \
  || { echo "tier golden: points budget at bucket 120 is not 1" >&2; exit 1; }

printf 'cold poll rows total=%d (due buckets 120,150 only): %s\n' "$cold_total" "OK"
printf 'hot poll rows total=%d (every clock tick): %s\n' "$hot_total" "OK"
printf 'GHCACHER_TIER_GOLDEN_HOLDS ticks=6 final=1\n'
