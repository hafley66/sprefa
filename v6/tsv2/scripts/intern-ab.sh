#!/usr/bin/env bash

# G9 (interning contract §15.4): compile the corpus twice at THIS commit, one
# mode each; every differing line must fall in an interning class.
set -euo pipefail
cd "$(dirname "$0")/.."

COMPILE_DIR="../prolog/compile"
COMPILE_OUT="$COMPILE_DIR/out"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

compile_at() {
  local mode="$1" dest="$2"
  mkdir -p "$dest"
  swipl -q -l "$COMPILE_DIR/../sweep.pl" \
        -g "sweep([intern($mode)])" -g halt > /dev/null
  cp -f "$COMPILE_OUT"/*.ts "$dest/"
}

compile_at dict "$WORK/dict"
compile_at direct "$WORK/direct"

set +e
node --experimental-transform-types scripts/intern-ab-classify.ts "$WORK/dict" "$WORK/direct"
STATUS=$?
set -e

# Leave out/ in the mode the corpus is checked in at.
swipl -q -l "$COMPILE_DIR/../sweep.pl" -g sweep -g halt > /dev/null
exit $STATUS
