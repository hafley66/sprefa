#!/usr/bin/env bash
# run.sh : compile every probe on the TEXT DOOR, one at a time, and print one
# row per probe. Buckets match sweep.pl's own: compiled / unsupported / crash.
# Emitted .ts lands in a scratch dir and is not kept.
#
# Run from anywhere:  bash v6/prolog/conformance/probes/2026-08-17-stress/run.sh

set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOOR="$HERE/../../../compile/scripts/compile_dl6.sh"
OUT="$HERE/.out"
mkdir -p "$OUT"

printf '%-38s %-12s %s\n' PROBE BUCKET REASON
for f in "$HERE"/[pn]*.dl6; do
  name="$(basename "$f" .dl6)"
  log="$OUT/$name.log"
  if bash "$DOOR" "$f" "$OUT/$name.ts" >"$log" 2>&1; then
    printf '%-38s %-12s %s\n' "$name" compiled ''
  else
    reason="$(grep -o 'unsupported_construct: .*' "$log" | head -1)"
    if [ -n "$reason" ]; then
      printf '%-38s %-12s %s\n' "$name" unsupported "${reason#unsupported_construct: }"
    else
      printf '%-38s %-12s %s\n' "$name" crash "$(head -3 "$log" | tr '\n' ' ')"
    fi
  fi
done
