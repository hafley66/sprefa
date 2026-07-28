#!/usr/bin/env bash
# Ratchet: exactly one manual rxjs .subscribe() in the app, ever. Target reached
# 2026-07-27 (standing plan item 3): the one site is dl/src/main.ts.
#
# dl/src/0_trace.ts's `sqlChannel.subscribe(...)` / `effectChannel.subscribe(...)` /
# `ingestChannel.subscribe(...)` (perf-tracing arc, 2026-07-27) are node:diagnostics_channel
# subscriptions, not rxjs Observable subscriptions -- a different API that happens to
# share the method name. They are excluded by path: 0_trace.ts is the one file this
# arc's design puts diagnostics_channel plumbing in, so excluding it here is precise,
# not a loophole. A future rxjs .subscribe() call added to that same file would still
# be invisible to this ratchet -- if 0_trace.ts ever grows one, tighten this filter to
# match Channel.subscribe( specifically instead of excluding the whole file.
set -euo pipefail
cd "$(dirname "$0")/.."
BASELINE=1
sites=$(grep -rn '\.subscribe(' dl/src --include='*.ts' | grep -v '^dl/src/0_trace\.ts:' || true)
count=$(printf '%s' "$sites" | grep -c . || true)
printf 'subscribe sites: %s (baseline %s, target 1)\n' "$count" "$BASELINE"
printf '%s\n' "$sites"
if [ "$count" -gt "$BASELINE" ]; then
  printf 'FAIL: a new manual subscription landed. Compose it into main.ts instead.\n' >&2
  exit 2
fi
[ "$count" -eq 1 ] && printf 'target reached; lower BASELINE to 1 in this script.\n'
exit 0
