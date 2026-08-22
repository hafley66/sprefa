#!/usr/bin/env bash
# Rust-door gate for the six ghcacher goldens. Scripted schedules, no network.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
engine="$root/v6/sprefa-engine-rs"
harness="$engine/target/debug/emit_rust_harness"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT INT TERM

cargo build --quiet --manifest-path "$engine/Cargo.toml" --bin emit_rust_harness 2>/dev/null

# The TS final line omits a zero-row rel; the Rust `--final` prints every rel in
# final_select. That is the whole shape difference and it is normalized here.
fold_final() {
  jq -cs '{final: (map(select(.rows | length > 0) | {key: .rel, value: .rows}) | from_entries)}'
}

names=(
  ghcacher_tick_golden
  ghcacher_304_golden
  ghcacher_config_golden
  ghcacher_env_golden
  ghcacher_checkout_golden
  ghcacher_tier_golden
)
failed=0

# One swipl process for all six: six startups cost more than the whole fold.
compile_goals=()
for name in "${names[@]}"; do
  compile_goals+=(-g "compile_dl6('$here/$name.dl6','$scratch/$name.rs',[emitter(emit_rust:emit_program)])")
done
compile_started=$(date +%s.%N)
swipl -q -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
  "${compile_goals[@]}" -g halt >"$scratch/compile.log" 2>&1
compile_code=$?
compile_wall=$(awk -v a="$compile_started" -v b="$(date +%s.%N)" 'BEGIN { printf "%.3f", b - a }')
if [ "$compile_code" != 0 ]; then
  echo "FAIL emit_rust compile:"
  tail -n 10 "$scratch/compile.log"
  exit 1
fi
printf 'emit_rust compile of 6 programs: %ss\n\n' "$compile_wall"

printf '%-26s %-6s %8s %10s %8s %8s\n' golden verdict wall_s statements rows_out changed

for name in "${names[@]}"; do
  program="$here/$name.dl6"
  schedule="$here/$name.schedule.json"
  expected_ticks="$here/$name.expected.tick.jsonl"
  expected_final="$here/$name.expected.final.jsonl"
  emitted="$scratch/$name.rs"

  # tsv2 door paused (user 2026-08-21): the arrival respelling diverges from
  # v6/tsv2/goldens on purpose, so the origin-diff step is gone.

  started=$(date +%s.%N)
  RUST_LOG=sprefa_engine_rs=info "$harness" "$emitted" "$schedule" --final \
    >"$scratch/$name.out" 2>"$scratch/$name.err"
  code=$?
  wall=$(awk -v a="$started" -v b="$(date +%s.%N)" 'BEGIN { printf "%.3f", b - a }')
  if [ "$code" != 0 ]; then
    printf '%-26s %-6s %8s\n' "$name" FAIL "$wall"
    echo "  runtime: $({ grep -m1 -A1 'panicked at' "$scratch/$name.err" || head -n1 "$scratch/$name.err"; } | tail -n1)"
    failed=1
    continue
  fi

  ticks=$(grep -c '^{"tick"' "$scratch/$name.out")
  head -n "$ticks" "$scratch/$name.out" >"$scratch/$name.ticks"
  tail -n +$((ticks + 1)) "$scratch/$name.out" | fold_final >"$scratch/$name.final"

  verdict=PASS
  if ! diff -q "$expected_ticks" "$scratch/$name.ticks" >/dev/null; then
    verdict=FAIL
    failed=1
    echo "FAIL $name first differing tick line:"
    diff -u "$expected_ticks" "$scratch/$name.ticks" | sed -n '3,8p'
  elif ! diff -q "$expected_final" "$scratch/$name.final" >/dev/null; then
    verdict=FAIL
    failed=1
    echo "FAIL $name final differs:"
    diff -u "$expected_final" "$scratch/$name.final" | sed -n '3,8p'
  fi

  tally=$(sed -e 's/\x1b\[[0-9;]*m//g' "$scratch/$name.err" \
    | grep -o 'statements=[0-9]* inert=[0-9]* inert_pct=[0-9]* rows_out=[0-9]* rows_changed=[0-9]*' \
    | tail -n 1)
  statements=$(printf '%s' "$tally" | sed -n 's/.*statements=\([0-9]*\).*/\1/p')
  rows_out=$(printf '%s' "$tally" | sed -n 's/.*rows_out=\([0-9]*\).*/\1/p')
  changed=$(printf '%s' "$tally" | sed -n 's/.*rows_changed=\([0-9]*\).*/\1/p')
  printf '%-26s %-6s %8s %10s %8s %8s\n' "$name" "$verdict" "$wall" \
    "${statements:-?}" "${rows_out:-?}" "${changed:-?}"
done

[ "$failed" = 0 ] && echo "GHCACHER_RUST_DOOR_HOLDS goldens=6"
exit "$failed"
