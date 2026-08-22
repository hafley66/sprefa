#!/usr/bin/env bash
# Rust-door gate for ghcache.dl6. Compile, then fold the scripted schedule.
set -uo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../../.." && pwd)"
engine="$root/v6/sprefa-engine-rs"
harness="$engine/target/debug/emit_rust_harness"
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT INT TERM

# The clock checker's path enumeration needs more than the default stack on a
# program this size; docs/failure-modes.md entry 63 carries the measurement.
stack="${GHCACHE_STACK_LIMIT:-12G}"
compile_budget="${GHCACHE_COMPILE_BUDGET:-900}"
fold_budget="${GHCACHE_FOLD_BUDGET:-60}"

cargo build --quiet --manifest-path "$engine/Cargo.toml" --bin emit_rust_harness || exit 1

started=$(date +%s)
timeout "$compile_budget" swipl --stack_limit="$stack" -q \
  -l "$root/v6/prolog/compile.pl" -l "$root/v6/prolog/emit_rust.pl" \
  -g "compile_dl6('$here/ghcache.dl6','$scratch/ghcache.rs',[emitter(emit_rust:emit_program)])" \
  -g halt >"$scratch/compile.log" 2>&1
compile_code=$?
compile_wall=$(( $(date +%s) - started ))

if [ "$compile_code" != 0 ] || [ ! -s "$scratch/ghcache.rs" ]; then
  echo "FAIL ghcache.dl6 did not compile in ${compile_wall}s at stack_limit=$stack"
  grep -m1 '"code"' "$scratch/compile.log" || tail -n 3 "$scratch/compile.log"
  echo "  see v6/dl/ghcache/README.md 'Status' and docs/failure-modes.md entry 63"
  exit 1
fi
printf 'compile: %ss at stack_limit=%s\n' "$compile_wall" "$stack"

started=$(date +%s)
DL_ADAPTERS_DIR="$here" RUST_LOG=sprefa_engine_rs=info \
  timeout "$fold_budget" "$harness" "$scratch/ghcache.rs" \
  "$here/ghcache.schedule.json" --final \
  >"$scratch/out" 2>"$scratch/err"
fold_code=$?
fold_wall=$(( $(date +%s) - started ))

if [ "$fold_code" != 0 ]; then
  echo "FAIL ghcache fold exited $fold_code after ${fold_wall}s"
  { grep -m1 -A1 'panicked at' "$scratch/err" || tail -n 3 "$scratch/err"; }
  exit 1
fi

ticks=$(grep -c '^{"tick"' "$scratch/out")
printf 'fold: %ss, %s ticks\n' "$fold_wall" "$ticks"

# COUNT RECEIPT for issues/ghcache-dl6-poll: a 60s period is ONE minute bucket,
# so three consecutive buckets are three polls, not one every sixty.
due_rows=$(jq -r 'select(.rel == "due") | .rows | length' "$scratch/out" | tail -n 1)
fresh=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 200)] | length' "$scratch/out" | tail -n 1)
cached=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 304)] | length' "$scratch/out" | tail -n 1)
cached_bytes=$(jq -r 'select(.rel == "call_log") | [.rows[] | select(.[3] == 304) | .[6]] | add // 0' "$scratch/out")
remaining=$(jq -r 'select(.rel == "call_log") | [.rows[] | .[4]] | min' "$scratch/out")
printf 'due=%s call_log 200=%s 304=%s 304_bytes=%s rate_remaining_min=%s\n' \
  "$due_rows" "$fresh" "$cached" "$cached_bytes" "$remaining"

fail=0
[ "$due_rows" = 3 ] || { echo "FAIL a 60s period over 3 buckets is 3 polls, got due=$due_rows"; fail=1; }
[ "$fresh" = 1 ] || { echo "FAIL the first poll is one 200, got $fresh"; fail=1; }
# Three due buckets and one re-ask: a 200 moves the stored tag, the tag is
# demand identity, so the bucket that changed asks once more, conditionally.
[ "$cached" = 3 ] || { echo "FAIL every later pass is a 304, got $cached"; fail=1; }
[ "$cached_bytes" = 0 ] || { echo "FAIL a 304 moves zero bytes, got $cached_bytes"; fail=1; }
[ "$remaining" = 4997 ] || { echo "FAIL the rate headers decode as ints, got $remaining"; fail=1; }
[ "$fail" = 0 ] || exit 1

echo "GHCACHE_RUST_DOOR_HOLDS ticks=$ticks"
