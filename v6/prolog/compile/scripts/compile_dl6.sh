#!/usr/bin/env bash
# compile_dl6.sh: compile one .dl6 source file through compile:compile_dl6/2.

set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 INPUT.dl6 OUTPUT.ts" >&2
  exit 2
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$HERE/.." && pwd)"
INPUT="$1"
OUTPUT="$2"

if [ -n "${DL_PERF_LOG:-}" ]; then
  swipl -q -l "$COMPILE_DIR/6_profile.pl" \
    -g "compile_dl6_profiled('$INPUT', '$OUTPUT')" \
    -g halt
else
  swipl -q -l "$COMPILE_DIR/compile.pl" \
    -g "compile_dl6('$INPUT', '$OUTPUT')" \
    -g halt
fi
