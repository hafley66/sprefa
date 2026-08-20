#!/usr/bin/env bash
# sweep-stage1.sh -- the compile sweep of scripts/sweep.sh stage 1, fanned out.
#
# Argument 1 is the worker count. One swipl per worker, each loading the
# compiler once (measured 0.2s, so the load is not what the fan-out is paying
# for) and compiling the fixtures whose corpus position is congruent to its
# index. sweep.pl's merge folds the fragments into the one manifest, in corpus
# order, which is the order the sequential door has always written.
#
# Worker stdout is buffered per rank and replayed in rank order so a sharded
# run prints the same lines in the same order every time; worker stderr stays
# on the terminal, so a worker in trouble says so while it is still in trouble.
set -euo pipefail
cd "$(dirname "$0")/.."

PROLOG_DIR="../prolog"
OUT="$PROLOG_DIR/compile/out"
SHARDS="$OUT/.sweep-shards"
JOBS="${1:-1}"

# SWEEP_FORCE=1 is the pre-pass clear_stale_compiled_outputs/1 used to run
# inside the prolog process on every sweep. It has to be a driver pre-pass and
# it has to be conditional: a worker that wiped the set would delete its peers'
# outputs, and an unconditional wipe would delete exactly what the digest cache
# exists to keep.
if [ "${SWEEP_FORCE:-0}" != "0" ]; then
  echo "SWEEP_FORCE=1: dropping every compiled output and the digest store"
  rm -f "$OUT"/*.ts "$OUT"/*.schedule.json "$OUT"/*.schema.json \
        "$OUT"/*.types.rs "$OUT/sweep.digests"
fi

rm -rf "$SHARDS"

if [ "$JOBS" -le 1 ]; then
  exec swipl -q -l "$PROLOG_DIR/sweep.pl" -g "compile_messages:dl6_debug_from_env" -g sweep -g halt
fi

mkdir -p "$SHARDS"

pids=()
for ((rank = 0; rank < JOBS; rank++)); do
  SWEEP_SHARD_INDEX="$rank" SWEEP_JOBS="$JOBS" \
    swipl -q -l "$PROLOG_DIR/sweep.pl" -g "compile_messages:dl6_debug_from_env" -g sweep_shard -g halt \
    > "$SHARDS/worker.$rank.log" &
  pids+=("$!")
done

status=0
for rank in "${!pids[@]}"; do
  if ! wait "${pids[$rank]}"; then
    status=1
    printf 'SWEEP_WORKER_FAILED rank=%s\n' "$rank" >&2
  fi
done

for ((rank = 0; rank < JOBS; rank++)); do
  [ -f "$SHARDS/worker.$rank.log" ] && cat "$SHARDS/worker.$rank.log"
done

if [ "$status" -ne 0 ]; then
  printf 'stage 1 kept its fragments at %s for reading\n' "$SHARDS" >&2
  exit "$status"
fi

SWEEP_JOBS="$JOBS" swipl -q -l "$PROLOG_DIR/sweep.pl" -g sweep_merge -g halt
