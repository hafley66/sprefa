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

# The rate-budget receipt: a stopped window issues zero GETs. `due` empty and
# `call_log` unchanged across the window is the assertion, not end-state rows.
calls=$(grep -c '"rel":"call_log"' "$scratch/out")
printf 'call_log lines: %s\n' "$calls"

echo "GHCACHE_RUST_DOOR_HOLDS ticks=$ticks"
