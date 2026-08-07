#!/usr/bin/env bash
# Exactly one manual rxjs `.subscribe()` per scanned app. Scanned apps are
# dl/src and tsv2/serve. Standalone one-shot CLI scripts are excluded.
#
# The diagnostics_channel handles in dl/src/0_trace.ts and tsv2/serve/0_trace.ts
# are subscribed by a different API that happens to share the method name, so
# they are excluded BY NAME -- the literal handle identifiers, never a
# `*Channel` wildcard. A wildcard let any rxjs `.subscribe()` on a variable whose
# name merely ended in "Channel" walk straight through (audit 2026-07-28, probe
# 2). Adding another diagnostics_channel handle means adding its name here, on
# purpose.
#
# Each count is floored at 1 so a missing scan path cannot pass silently.
set -euo pipefail
cd "$(dirname "$0")/.."

TRACE_CHANNEL_HANDLES='(sqlChannel|effectChannel|bindChannel|watchChannel|ingestChannel|tickChannel|ruleChannel|sql_channel|effect_channel|bind_channel|watch_channel|ingest_channel|tick_channel|rule_channel)\.subscribe\('
status=0

check_app() {
  local path="$1" baseline="$2" entry="$3"
  local sites count
  sites=$(grep -rn '\.subscribe(' "$path" --include='*.ts' | grep -Ev "$TRACE_CHANNEL_HANDLES" || true)
  count=$(printf '%s' "$sites" | grep -c . || true)
  printf '%s subscribe sites: %s (baseline %s, target 1)\n' "$path" "$count" "$baseline"
  printf '%s\n' "$sites"
  if [ "$count" -lt 1 ]; then
    printf 'FAIL: zero subscribe sites in %s -- %s should hold exactly one. The scan\n' "$path" "$entry" >&2
    printf '      path most likely stopped matching any source.\n' >&2
    status=2
    return
  fi
  if [ "$count" -gt "$baseline" ]; then
    printf 'FAIL: a new manual subscription landed in %s. Compose it into %s instead.\n' "$path" "$entry" >&2
    status=2
  fi
}

check_app dl/src 1 dl/src/main.ts
check_app tsv2/serve 1 tsv2/serve/main.ts

exit "$status"
