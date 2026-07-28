#!/usr/bin/env bash
# Ratchet: exactly one manual rxjs .subscribe() in the app, ever. Target reached
# 2026-07-27 (standing plan item 3): the one site is dl/src/main.ts.
#
# dl/src/0_trace.ts's four node:diagnostics_channel handles are subscribed by a
# different API that happens to share the method name, so they are excluded BY NAME --
# the four literal handle identifiers, never a `*Channel` wildcard. A wildcard let any
# rxjs `.subscribe()` on a variable whose name merely ended in "Channel" walk straight
# through the ratchet (audit 2026-07-28, probe 2). Adding a fifth diagnostics_channel
# handle means adding its name here, on purpose.
#
# The count is also floored at 1: a zero means the scanned path stopped matching any
# source (a rename, a moved package), which used to read as a silent pass (probe 3).
set -euo pipefail
cd "$(dirname "$0")/.."
BASELINE=1
TRACE_CHANNEL_HANDLES='(sqlChannel|effectChannel|bindChannel|ingestChannel)\.subscribe\('
sites=$(grep -rn '\.subscribe(' dl/src --include='*.ts' | grep -Ev "$TRACE_CHANNEL_HANDLES" || true)
count=$(printf '%s' "$sites" | grep -c . || true)
printf 'subscribe sites: %s (baseline %s, target 1)\n' "$count" "$BASELINE"
printf '%s\n' "$sites"
if [ "$count" -lt 1 ]; then
  printf 'FAIL: zero subscribe sites found -- main.ts should hold exactly one. The scan\n' >&2
  printf '      path (dl/src) most likely stopped matching any source.\n' >&2
  exit 2
fi
if [ "$count" -gt "$BASELINE" ]; then
  printf 'FAIL: a new manual subscription landed. Compose it into main.ts instead.\n' >&2
  exit 2
fi
exit 0
