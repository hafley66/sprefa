#!/usr/bin/env bash

# DL6_BENCH_FULL=1 adds layered and chain at 10k (about 35s more).

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SHOOTOUT="$(cd "$HERE/.." && pwd)"
COMPILE_DL6="$HERE/../../../prolog/compile/scripts/compile_dl6.sh"
MODULE_DIR="$HERE/.compiled"
MODULE="$MODULE_DIR/reachability.ts"
WORK="${DL6_BENCH_WORK:-$HERE/.bench}"
FACTS="$HERE/FACTS.md"

mkdir -p "$MODULE_DIR" "$WORK"
[ -L "$HERE/runtime" ] || ln -s "../../../tsv2/runtime" "$HERE/runtime"
[ -e "$HERE/node_modules" ] || ln -s "../../../tsv2/node_modules" "$HERE/node_modules"

compile_start=$(node -e 'process.stdout.write(String(Date.now()))')
if ! bash "$COMPILE_DL6" "$HERE/reachability.dl6" "$MODULE" >"$MODULE_DIR/compile.log" 2>&1; then
  cat "$MODULE_DIR/compile.log" >&2
  echo "dl6-bench: compile failed" >&2
  exit 1
fi
compile_end=$(node -e 'process.stdout.write(String(Date.now()))')
echo "dl6-bench: compiled in $((compile_end - compile_start))ms" >&2

HARNESS="$SHOOTOUT/harness/target/release/harness"
if [ ! -x "$HARNESS" ]; then
  echo "dl6-bench: building the input generator" >&2
  (cd "$SHOOTOUT/harness" && cargo build --release --quiet) || exit 1
fi

# The harness rewrites STANDINGS.md from its own binary path, so bank it first.
if [ ! -f "$WORK/grid_10000.in" ]; then
  echo "dl6-bench: generating inputs" >&2
  cp "$SHOOTOUT/STANDINGS.md" "$WORK/STANDINGS.banked" 2>/dev/null
  "$HARNESS" --engines ref --scales 10000 --work "$WORK" >/dev/null 2>&1
  [ -f "$WORK/STANDINGS.banked" ] && mv "$WORK/STANDINGS.banked" "$SHOOTOUT/STANDINGS.md"
fi

CASES=("grid_10000=$WORK/grid_10000.in")
if [ "${DL6_BENCH_FULL:-0}" = "1" ]; then
  CASES+=("layered_10000=$WORK/layered_10000.in" "chain_10000=$WORK/chain_10000.in")
fi

node --experimental-transform-types "$HERE/bench.ts" "$MODULE" "${CASES[@]}" >"$FACTS"
status=$?
if [ "$status" -ne 0 ]; then
  echo "dl6-bench: run failed" >&2
  exit "$status"
fi

echo "dl6-bench: wrote $FACTS" >&2
sed -n '/## Contract numbers/,/^$/p;/^| \`/p' "$FACTS" | head -20
