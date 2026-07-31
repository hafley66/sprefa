#!/usr/bin/env bash
# 1_compile_speed.sh -- ratcheted acceptance gate on .dl6 COMPILER cost.
#
# WHY INFERENCES AND NOT MILLISECONDS
# The compiler already regressed once in exactly the way a wall-clock gate
# cannot catch: clock_check's SCC ran all-pairs simple-path enumeration and
# flagship-flow.dl6 compiled in 255 s (plan phase 256,950 ms, 6,011,087,004
# inferences). Nothing failed, because nothing measured it. A wall-clock gate
# tight enough to catch that on a fast machine flakes on a loaded one, so it
# gets loosened until it catches nothing.
#
# SWI's inference counter is the deterministic meter. Measured on this corpus,
# three consecutive runs of all four pinned programs produced BYTE-IDENTICAL
# per-phase counts (flagship-flow 2,120,677 three times; wall over the same
# runs moved 7 -> 11 ms on the smallest program). So the gate reads inferences
# and prints wall milliseconds as INFORMATIONAL ONLY, never gated.
#
# THE RATCHET (prolog-lint.sh idiom: it only moves down, and only on purpose)
#   measured > baseline * 1.10   REGRESSION, exit 1
#   measured < baseline * 0.75   IMPROVED,   exit 1 with the --write-baseline
#                                instruction, so a win is banked deliberately
#                                rather than silently re-opening headroom
#   otherwise                    OK
# The bands are asymmetric on purpose. Counts are exactly reproducible, so the
# +10% is pure headroom for semantically-neutral edits that shift counts a
# little; the -25% only trips on a real win, which keeps a lab that shaves a
# few percent off the plan phase from turning `just green-all` red.
#
# DIVERGENCE FROM prolog-lint.sh, stated: that gate compares finding SETS and
# this one compares NUMBERS, so "no longer present" becomes "materially
# smaller". It does NOT rewrite the baseline during a gate run -- a gate that
# edits a checked-in file leaves `just green-all` with a dirty tree.
#
# PINNED PROGRAMS: the four v6/dl/fixtures/*.dl6 that compile clean.
# ghcacher.dl6 and conformance.dl6 are deliberately NOT pinned -- they exit on
# named refusals today (recursive_stratum and level_rule_no_positive_body
# respectively), so they measure the refusal path, not the compile path.
#
# SABOTAGE RECEIPT (run 2026-07-30, reverted; git status clean after):
#   Inserted one artificial linear scan into the body walk every rule passes
#   through, compile/analyze.pl body_ref_uses/2:
#       ( numlist(1, 400, SabotageScan), member(_, SabotageScan), fail ; true ),
#   The gate went RED and named both the phase and the program:
#     flagship-flow        plan    1369399 -> 13691527   (+899.8%)  REGRESSION
#     golden-flex          plan     359646 ->  4338062  (+1106.2%)  REGRESSION
#     flagship-callgraph   plan      92220 ->   889354   (+864.4%)  REGRESSION
#     door-handwritten     plan      13155 ->    86501   (+557.6%)  REGRESSION
#     ... lower +72..263%, emit +13..31% on all four
#     COMPILE_SPEED regressions=12 improvements=0 FAIL   (exit 1)
#   Worth noting against the wall-clock alternative: the same sabotage moved
#   door-handwritten's wall from 7 ms to 12 ms, a 5 ms difference that no
#   honest wall-clock threshold would gate. The inference counter called it
#   +557.6% on the same run.
#   Reverted with `git checkout -- analyze.pl`; gate green again at
#   COMPILE_SPEED programs=4 phases=24 regressions=0 improvements=0 OK.
#
# Refresh the baseline after a deliberate change:
#   bash scripts/1_compile_speed.sh --write-baseline

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
compile_dir="$(cd "$here/.." && pwd)"
v6_dir="$(cd "$compile_dir/../.." && pwd)"
baseline="$here/compile-speed-baseline.tsv"

# Regression headroom above baseline, and the improvement floor below it.
regression_ratio=1.10
improvement_ratio=0.75

pinned_programs="flagship-flow golden-flex flagship-callgraph door-handwritten"
phases="parse plan lower boot emit write"

write_baseline=0
if [ "${1:-}" = "--write-baseline" ]; then
  write_baseline=1
fi

scratch="$(mktemp -d "${TMPDIR:-/tmp}/sprefa-compile-speed.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

# Hermetic: no daemon, no shared state dir, no ambient config.
export SPREFA_CONFIG=/nonexistent/x.toml
export DL_NO_DAEMON=1
export DL_DB_PATH="$scratch/scratch.sqlite"
export XDG_STATE_HOME="$scratch/state"

measured="$scratch/measured.tsv"
: > "$measured"
wall_report="$scratch/wall.tsv"
: > "$wall_report"

cd "$compile_dir"

for program in $pinned_programs; do
  source_file="$v6_dir/dl/fixtures/$program.dl6"
  if [ ! -f "$source_file" ]; then
    echo "compile-speed: pinned program missing: $source_file" >&2
    exit 1
  fi

  log_file="$scratch/$program.jsonl"
  : > "$log_file"

  if ! DL_PERF_LOG="$log_file" bash "$here/compile_dl6.sh" \
         "$source_file" "$scratch/$program.ts" \
         > "$scratch/$program.out" 2> "$scratch/$program.err"; then
    echo "compile-speed: $program failed to compile" >&2
    tail -5 "$scratch/$program.err" >&2
    exit 1
  fi

  for phase in $phases; do
    count="$(jq -s -r --arg phase "$phase" \
      '[.[] | select(.phase == $phase) | .inferences] | add // 0' "$log_file")"
    printf '%s\t%s\t%s\n' "$program" "$phase" "$count" >> "$measured"
  done

  wall="$(jq -s -r '[.[] | .wall_ms] | add' "$log_file")"
  total="$(jq -s -r '[.[] | .inferences] | add' "$log_file")"
  printf '%s\t%s\t%s\n' "$program" "$wall" "$total" >> "$wall_report"
done

if [ "$write_baseline" = "1" ]; then
  {
    echo "# compile-speed baseline: SWI inference counts per .dl6 program per"
    echo "# compiler phase. Regenerate with:"
    echo "#   bash scripts/1_compile_speed.sh --write-baseline"
    echo "# Gate bands: fail above baseline*$regression_ratio, fail below"
    echo "# baseline*$improvement_ratio (bank the win deliberately)."
    echo "# Written $(date -u '+%Y-%m-%dT%H:%M:%SZ') on $(uname -m) $(swipl --version)"
    echo "# program	phase	inferences"
    cat "$measured"
  } > "$baseline"
  echo "compile-speed: baseline written, $(wc -l < "$measured" | tr -d ' ') rows"
  exit 0
fi

if [ ! -f "$baseline" ]; then
  echo "compile-speed: no baseline at $baseline" >&2
  echo "run: bash scripts/1_compile_speed.sh --write-baseline" >&2
  exit 1
fi

printf '%-20s %-7s %12s %12s %9s  %s\n' \
  program phase baseline measured delta verdict

status=0
regressions=0
improvements=0
phase_rows=0
worst_program=
worst_phase=
worst_ratio=0

while IFS=$'\t' read -r program phase count; do
  [ -n "$program" ] || continue
  base="$(awk -F '\t' -v p="$program" -v ph="$phase" \
    '$1 == p && $2 == ph { print $3 }' "$baseline")"

  if [ -z "$base" ]; then
    printf '%-20s %-7s %12s %12s %9s  %s\n' \
      "$program" "$phase" "-" "$count" "-" "NEW (not in baseline)"
    status=1
    continue
  fi

  phase_rows=$((phase_rows + 1))
  read -r delta verdict <<< "$(awk -v base="$base" -v got="$count" \
    -v hi="$regression_ratio" -v lo="$improvement_ratio" 'BEGIN {
      if (base == 0) { printf "n/a %s", (got == 0 ? "OK" : "REGRESSION"); exit }
      ratio = got / base
      printf "%+.1f%% ", (ratio - 1) * 100
      if (ratio > hi) print "REGRESSION"
      else if (ratio < lo) print "IMPROVED"
      else print "OK"
    }')"

  case "$verdict" in
    REGRESSION)
      regressions=$((regressions + 1))
      status=1
      candidate_ratio="$(awk -v base="$base" -v got="$count" 'BEGIN {
        if (base == 0) print (got == 0 ? 0 : 1e300)
        else print got / base
      }')"
      if [ -z "$worst_program" ] || awk -v candidate="$candidate_ratio" \
          -v current="$worst_ratio" 'BEGIN { exit !(candidate > current) }'; then
        worst_program="$program"
        worst_phase="$phase"
        worst_ratio="$candidate_ratio"
      fi
      ;;
    IMPROVED)   improvements=$((improvements + 1)); status=1 ;;
  esac

  printf '%-20s %-7s %12s %12s %9s  %s\n' \
    "$program" "$phase" "$base" "$count" "$delta" "$verdict"
done < "$measured"

echo
echo "── informational, NOT gated: wall ms and total inferences per program ──"
printf '%-20s %10s %14s\n' program wall_ms total_inferences
while IFS=$'\t' read -r program wall total; do
  printf '%-20s %10s %14s\n' "$program" "$wall" "$total"
done < "$wall_report"
echo

if [ "$regressions" -gt 0 ]; then
  profile_source="$v6_dir/dl/fixtures/$worst_program.dl6"
  profile_output="$scratch/$worst_program.profile.txt"
  profile_top="$scratch/$worst_program.profile.top.txt"
  profile_destination="$scratch/$worst_program.profile.ts"
  echo "COMPILE_PROFILE program=$worst_program phase=$worst_phase top_self_time_lines=15"
  if "$v6_dir/tools/run-capped.sh" 120 swipl -q \
      -l "$compile_dir/6_profile.pl" \
      -g "compile_profile:execution_profile_dl6('$profile_source', '$profile_destination')" \
      -g halt >"$profile_output" 2>&1; then
    awk '
      /^Predicate[[:space:]]/ { collecting = 1; next }
      collecting && /^=+/ { next }
      collecting && NF {
        print
        lines++
        if (lines == 15) exit
      }
    ' "$profile_output" > "$profile_top"
    echo "COMPILE_PROFILE_TOP_SELF_TIME_BEGIN"
    sed -n '1,15p' "$profile_top"
    echo "COMPILE_PROFILE_TOP_SELF_TIME_END"
  else
    profile_status=$?
    echo "COMPILE_PROFILE status=failed exit=$profile_status"
    sed -n '1,20p' "$profile_output"
  fi
fi

if [ "$improvements" -gt 0 ] && [ "$regressions" -eq 0 ]; then
  echo "Compiler got materially FASTER. Bank it:"
  echo "  bash scripts/1_compile_speed.sh --write-baseline"
  echo
fi

program_count="$(echo $pinned_programs | wc -w | tr -d ' ')"
if [ "$status" = "0" ]; then
  echo "COMPILE_SPEED programs=$program_count phases=$phase_rows regressions=0 improvements=0 OK"
else
  echo "COMPILE_SPEED regressions=$regressions improvements=$improvements FAIL"
fi
exit "$status"
