#!/usr/bin/env bash
# exec_shootout dl6 entrypoint: compile through the v6 pipeline, run the module.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DL6="$HERE/../../../prolog/compile/scripts/compile_dl6.sh"
DL6_SRC="$HERE/reachability.dl6"
MODULE_DIR="$HERE/.compiled"
MODULE="$MODULE_DIR/reachability.ts"
mkdir -p "$MODULE_DIR"

INPUT=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --input)   INPUT="$2";  shift 2 ;;
    --threads) shift 2 ;;
    *) echo "dl6: unknown flag $1" >&2; exit 1 ;;
  esac
done

if [ -z "$INPUT" ]; then
  echo "dl6: --input <path> is required" >&2
  exit 1
fi

# Resolve the committed runtime and pnpm dependency set as sibling symlinks,
# the same way bench-cli does, never vendored copies.
[ -L "$HERE/runtime" ] || ln -s "../../../tsv2/runtime" "$HERE/runtime"
[ -e "$HERE/node_modules" ] || ln -s "../../../tsv2/node_modules" "$HERE/node_modules"

COMPILE_LOG="$MODULE_DIR/compile.log"
compile_start=$(node -e 'process.stdout.write(String(Date.now()))')
bash "$COMPILE_DL6" "$DL6_SRC" "$MODULE" >"$COMPILE_LOG" 2>&1
compile_status=$?
compile_end=$(node -e 'process.stdout.write(String(Date.now()))')
COMPILE_MS=$((compile_end - compile_start))

if [ "$compile_status" -ne 0 ]; then
  cat "$COMPILE_LOG" >&2
  exit 1
fi

echo "dl6: compile_ms=$COMPILE_MS" >&2
node --experimental-transform-types "$HERE/run.ts" --input "$INPUT" --module "$MODULE"
exit $?
