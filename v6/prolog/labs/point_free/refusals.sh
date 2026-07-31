#!/usr/bin/env bash
# refusals.sh : print the refusal every break/ sugar file earns, one per line.
set -u
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$HERE"
for file in break/*.sugar.pl; do
  printf '%-42s ' "$file"
  swipl -q -l emit.pl -g "show_refusal('$file')" -g halt 2>&1 | tail -1
done
