#!/usr/bin/env bash
# compile_dl6.sh: compile one .dl6 source file through compile:compile_dl6/2.

set -euo pipefail

INTERN=""
ARGS=()
for arg in "$@"; do
  case "$arg" in
    --intern=*) INTERN="${arg#--intern=}" ;;
    *) ARGS+=("$arg") ;;
  esac
done

if [ "${#ARGS[@]}" -ne 2 ]; then
  echo "usage: $0 [--intern=dict|direct] INPUT.dl6 OUTPUT.ts" >&2
  exit 2
fi

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPILE_DIR="$(cd "$HERE/.." && pwd)"
INPUT="${ARGS[0]}"
OUTPUT="${ARGS[1]}"

if [ -n "${DL_PERF_LOG:-}" ]; then
  swipl -q -l "$COMPILE_DIR/../6_profile.pl" \
    -g "compile_dl6_profiled('$INPUT', '$OUTPUT')" \
    -g halt
else
  if [ -n "$INTERN" ]; then
    GOAL="compile_dl6('$INPUT', '$OUTPUT', [intern($INTERN)])"
  else
    GOAL="compile_dl6('$INPUT', '$OUTPUT')"
  fi
  swipl -q -l "$COMPILE_DIR/../compile.pl" -g "$GOAL" -g halt
fi
