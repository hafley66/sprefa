#!/usr/bin/env bash
# Ratchet: exactly one manual .subscribe() in the app, ever. Target reached
# 2026-07-27 (standing plan item 3): the one site is dl/src/main.ts.
set -euo pipefail
cd "$(dirname "$0")/.."
BASELINE=1
sites=$(grep -rn '\.subscribe(' dl/src --include='*.ts' || true)
count=$(printf '%s' "$sites" | grep -c . || true)
printf 'subscribe sites: %s (baseline %s, target 1)\n' "$count" "$BASELINE"
printf '%s\n' "$sites"
if [ "$count" -gt "$BASELINE" ]; then
  printf 'FAIL: a new manual subscription landed. Compose it into main.ts instead.\n' >&2
  exit 2
fi
[ "$count" -eq 1 ] && printf 'target reached; lower BASELINE to 1 in this script.\n'
exit 0
