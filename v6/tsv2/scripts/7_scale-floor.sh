#!/usr/bin/env bash
# 7_scale-floor.sh -- acceptance floor for tsv2 throughput at scale.
#
# WHAT WAS MISSING
# memory-soak proves memory stays flat and serve-leak-soak proves handles stay
# flat, but NOTHING failed if throughput cratered. The scale bench (SCALE.md)
# measured a superlinear curve and an OOM and both were written down rather
# than gated. This is the gate.
#
# THE RIG IS BORROWED, NOT INVENTED
# Two existing runners do all the work, unmodified:
#   scripts/1_p1-receipts.ts  statements per tick via stmt_counter, EXPLAIN
#                             plans, incrementalSafe -- the deterministic half
#   scripts/scale-bench.ts    wall, ticks, arrivals, final row counts, RSS
#                             -- the informational half
# and the fixture comes from sprefa-store/bench/scale-gen.pl + compile_fixture/4,
# exactly as bench/engines/tsv2_gen.sh drives them.
#
# WHAT IS GATED, AND WHY THE SPLIT
# Deterministic counters are gated EXACTLY:
#   * the SET of statements-per-tick values must equal the pinned set AND must
#     be IDENTICAL at two cell sizes 10x apart. Measured here, s2 runs 37 or 41
#     statements per tick and that set does not move between 1,000 and 20,000
#     arrivals -- 41 is fixed first-tick overhead, not growth. Comparing the
#     set across two sizes is what actually proves delta-proportionality; a
#     single-size assertion cannot tell fixed overhead from a per-row loop.
#     The moment an arrival-row loop or full-table rescan returns, the larger
#     cell's set moves and the small cell's does not, and this goes red. This
#     is the standing count-test law applied to a formerly-quadratic path.
#   * tick count and final row counts are exact.
#   * incrementalSafe must hold -- a silent fall back to the naive snapshot
#     path is the most likely way throughput craters without anything erroring.
# Wall time is gated with a deliberately loose 3x floor, because CI-machine
# variance and a shared laptop are real and a tight wall gate gets loosened
# until it catches nothing. 3x still catches a cratering (the naive path is
# ~165x slower on this very cell), which is the failure this gate is for.
#
# EVERY RUN APPENDS to goldens/scale-floor-history.jsonl so the curve exists
# over time even on runs that pass.
#
# THE TRACKED FILE IS BORROWED AND PUT BACK
# Both runners statically import ../gen/scale_generated.ts, which is tracked
# and currently holds the s1 program. This gate needs the s2 program there, so
# it saves the bytes, swaps, runs, and restores under a trap, then FAILS LOUDLY
# if the restore did not produce byte-identical content. The emitted program is
# independent of the row count (verified: s1/10000 and s1/1000 compile
# byte-identical, likewise s2), so only the shape matters here.
#
# FINDING, not fixed by this lane: the checked-in gen/scale_generated.ts is
# already STALE against a fresh s1 compile -- it predates the host/bind/query
# plan interfaces. That is the gen-module staleness class (ARCH gen_staleness_gate,
# the one door-handwritten.ts hit four times). This gate deliberately restores
# the stale bytes rather than silently "fixing" a file it does not own.
#
# SABOTAGE RECEIPTS (run 2026-07-30, both reverted; tree clean after)
#   1. Forced the naive snapshot emitter with SPREFA_TSV2_EMITTER_MODE=naive.
#      That is the real shape of a cratered throughput regression -- the
#      incremental plan silently replaced by a recompute-per-tick one:
#        stmts/tick set @10000   [37,41]  ->  [114]         FAIL
#        ms/1k arrivals          <=44.537 ->   189.2251292  FAIL  (12.7x)
#        worker_rss_mb            164.8   ->   303.1
#        SCALE_FLOOR s2 rows=10000 FAIL   (exit 1)
#      Note the control cell agreed with itself at [114]: naive is uniformly
#      worse, not size-dependent. The pinned-set leg is what caught it.
#   2. Perturbed ONLY the expected set, 37 -> 36, leaving the clock alone:
#        stmts/tick set @10000   [36,41]  ->  [37,41]       FAIL
#        ms/1k arrivals          <=44.537 ->    14.9988292  OK
#      The count leg goes red with the wall leg green, so the deterministic
#      half is not piggybacking on timing.
#
# Re-pin the reference numbers after a deliberate change:
#   SCALE_FLOOR_WRITE=1 bash scripts/7_scale-floor.sh

set -uo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tsv2_dir="$(cd "$here/.." && pwd)"
v6_dir="$(cd "$tsv2_dir/.." && pwd)"
bench_dir="$v6_dir/sprefa-store/bench"
generated="$tsv2_dir/gen/scale_generated.ts"
history="$tsv2_dir/goldens/scale-floor-history.jsonl"
reference="$here/scale-floor-reference.json"

shape="${SCALE_FLOOR_SHAPE:-s2}"
rows="${SCALE_FLOOR_ROWS:-10000}"
# The control cell exists only to prove the statement-count set does not move
# with size. Kept 10x smaller so it costs a second.
control_rows="${SCALE_FLOOR_CONTROL_ROWS:-1000}"
# Wall multiplier. Loose on purpose: see the header.
wall_multiplier="${SCALE_FLOOR_WALL_MULTIPLIER:-3}"
write_reference="${SCALE_FLOOR_WRITE:-0}"

scratch="$(mktemp -d "${TMPDIR:-/tmp}/sprefa-scale-floor.XXXXXX")"
saved="$scratch/scale_generated.ts.original"
cp "$generated" "$saved"

restore_generated() {
  if [ -f "$saved" ]; then
    cp "$saved" "$generated"
  fi
  rm -rf "$scratch"
}
trap restore_generated EXIT

echo "── scale floor: $shape at $rows arrivals through the tsv2 emitter ──"

# 1. fixture + compile, exactly as bench/engines/tsv2_gen.sh does it
fixture="$scratch/$shape-$rows.pl"
if ! swipl -q -s "$bench_dir/scale-gen.pl" \
     -g "scale_gen:write_fixture($shape,$rows,'$fixture'),halt" \
     > /dev/null 2> "$scratch/gen.err"; then
  echo "scale-floor: fixture generation failed for $shape/$rows" >&2
  cat "$scratch/gen.err" >&2
  exit 1
fi

if ! swipl -q -l "$v6_dir/prolog/compile/compile.pl" \
     -g "compile_fixture(scale_bench,'$fixture','$generated',emit_ts:emit_program),halt" \
     > /dev/null 2> "$scratch/compile.err"; then
  echo "scale-floor: compile_fixture/4 failed for $shape/$rows" >&2
  tail -5 "$scratch/compile.err" >&2
  exit 1
fi

cd "$tsv2_dir"

# 2. deterministic leg: statements per tick, plans, incrementalSafe.
# The same emitted module serves both cells -- the program does not depend on
# the row count, only the schedule does.
run_receipts() {
  local cell_rows="$1" destination="$2"
  if ! node --experimental-transform-types "$here/1_p1-receipts.ts" \
       "$shape" "$cell_rows" > "$destination" 2> "$scratch/receipt-$cell_rows.err"; then
    echo "scale-floor: p1 receipts failed for $shape/$cell_rows" >&2
    tail -20 "$scratch/receipt-$cell_rows.err" >&2
    exit 1
  fi
}

receipt="$scratch/receipt.json"
control_receipt="$scratch/receipt-control.json"
run_receipts "$rows" "$receipt"
run_receipts "$control_rows" "$control_receipt"

statements_per_tick_set="$(jq -c '.statementCounts' "$receipt")"
control_statements_set="$(jq -c '.statementCounts' "$control_receipt")"
tick_count="$(jq -r '.tickCount' "$receipt")"
incremental_safe="$(jq -r '.incrementalSafe' "$receipt")"

# 3. informational leg: wall, RSS, final row counts
record="$scratch/scale.jsonl"
: > "$record"
if ! node --experimental-transform-types "$here/scale-bench.ts" \
     "$shape" "$rows" "$record" > "$scratch/bench.out" 2> "$scratch/bench.err"; then
  echo "scale-floor: scale bench failed for $shape/$rows" >&2
  tail -20 "$scratch/bench.err" >&2
  exit 1
fi

total_wall_ms="$(jq -r '.total_wall_ms' "$record")"
ms_per_1k="$(jq -r '.ms_per_1k_arrivals' "$record")"
rss_mb="$(jq -r '.worker_rss_mb' "$record")"
host_peak_mb="$(jq -r '.host_peak_mb' "$record")"
arrivals="$(jq -r '.arrivals' "$record")"
final_rows="$(jq -r '[.final_table_sizes[]] | add' "$record")"

if [ "$write_reference" = "1" ]; then
  jq -n \
    --arg shape "$shape" \
    --argjson rows "$rows" \
    --argjson control_rows "$control_rows" \
    --argjson statements_per_tick_set "$statements_per_tick_set" \
    --argjson tick_count "$tick_count" \
    --argjson arrivals "$arrivals" \
    --argjson final_rows "$final_rows" \
    --argjson ms_per_1k_arrivals "$ms_per_1k" \
    '{shape: $shape, rows: $rows, control_rows: $control_rows,
      statements_per_tick_set: $statements_per_tick_set,
      tick_count: $tick_count, arrivals: $arrivals, final_rows: $final_rows,
      ms_per_1k_arrivals_reference: $ms_per_1k_arrivals,
      note: "The statements-per-tick SET is an exact gate, and must be the same at rows and control_rows -- that equality across sizes is the delta-proportionality proof, not the set value itself. ticks, arrivals and final rows are exact. ms_per_1k_arrivals_reference is multiplied by SCALE_FLOOR_WALL_MULTIPLIER (default 3) before comparison; it is a floor against cratering, not a performance target."}' \
    > "$reference"
  echo "scale-floor: reference written to $reference"
  cat "$reference"
  exit 0
fi

if [ ! -f "$reference" ]; then
  echo "scale-floor: no reference at $reference" >&2
  echo "run: SCALE_FLOOR_WRITE=1 bash scripts/7_scale-floor.sh" >&2
  exit 1
fi

expected_statements_set="$(jq -c '.statements_per_tick_set' "$reference")"
expected_ticks="$(jq -r '.tick_count' "$reference")"
expected_arrivals="$(jq -r '.arrivals' "$reference")"
expected_final_rows="$(jq -r '.final_rows' "$reference")"
reference_ms_per_1k="$(jq -r '.ms_per_1k_arrivals_reference' "$reference")"
wall_ceiling="$(awk -v r="$reference_ms_per_1k" -v m="$wall_multiplier" \
  'BEGIN { printf "%.3f", r * m }')"

status=0
check() {
  local label="$1" expected="$2" got="$3" verdict
  if [ "$expected" = "$got" ]; then verdict=OK; else verdict=FAIL; status=1; fi
  printf '%-26s %16s %16s   %s\n' "$label" "$expected" "$got" "$verdict"
}

printf '%-26s %16s %16s   %s\n' "gated counter" expected measured verdict
check "stmts/tick set @$rows"  "$expected_statements_set" "$statements_per_tick_set"
check "same set @$control_rows" "$statements_per_tick_set" "$control_statements_set"
check "ticks"                  "$expected_ticks"       "$tick_count"
check "arrivals"               "$expected_arrivals"    "$arrivals"
check "final rows"             "$expected_final_rows"  "$final_rows"
check "incrementalSafe"        true                    "$incremental_safe"

wall_verdict="$(awk -v got="$ms_per_1k" -v ceiling="$wall_ceiling" \
  'BEGIN { print (got > ceiling) ? "FAIL" : "OK" }')"
[ "$wall_verdict" = "OK" ] || status=1
printf '%-26s %16s %16s   %s\n' \
  "ms/1k arrivals (<=${wall_multiplier}x)" "$wall_ceiling" "$ms_per_1k" "$wall_verdict"

if [ "$statements_per_tick_set" != "$control_statements_set" ]; then
  echo
  echo "statements per tick MOVED with size: $control_rows -> $control_statements_set"
  echo "                                    $rows -> $statements_per_tick_set"
  echo "The emitted plan stopped being delta-proportional."
fi

echo
echo "── informational, NOT gated ──"
printf 'total_wall_ms=%s  worker_rss_mb=%s  host_peak_mb=%s  reference_ms_per_1k=%s\n' \
  "$total_wall_ms" "$rss_mb" "$host_peak_mb" "$reference_ms_per_1k"

mkdir -p "$(dirname "$history")"
jq -c -n \
  --arg at "$(date -u '+%Y-%m-%dT%H:%M:%SZ')" \
  --arg commit "$(git -C "$v6_dir" rev-parse --short HEAD 2>/dev/null || echo unknown)" \
  --arg shape "$shape" \
  --argjson rows "$rows" \
  --argjson statements_per_tick_set "$statements_per_tick_set" \
  --argjson ticks "$tick_count" \
  --argjson total_wall_ms "$total_wall_ms" \
  --argjson ms_per_1k_arrivals "$ms_per_1k" \
  --argjson worker_rss_mb "$rss_mb" \
  --argjson host_peak_mb "$host_peak_mb" \
  --arg verdict "$([ "$status" = 0 ] && echo pass || echo fail)" \
  '$ARGS.named' >> "$history"

# The borrowed tracked file must go back exactly as it was.
cp "$saved" "$generated"
if ! cmp -s "$saved" "$generated"; then
  echo "scale-floor: FAILED to restore $generated" >&2
  exit 1
fi

echo
if [ "$status" = "0" ]; then
  echo "SCALE_FLOOR $shape rows=$rows stmts/tick=$statements_per_tick_set flat-vs-$control_rows ticks=$tick_count ms/1k=$ms_per_1k OK"
else
  echo "SCALE_FLOOR $shape rows=$rows FAIL"
fi
exit "$status"
