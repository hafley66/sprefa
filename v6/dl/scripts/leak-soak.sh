#!/usr/bin/env bash
# Long-running node-server leak gate. The assertions and receipts live in the
# supporting test so the server process can inspect its own active resources.
#
# SABOTAGE RECEIPT: commenting out `sseRequest.destroy()` produced:
#   FAIL  timeout waiting for SSE teardown iteration=1
#
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 1s at the default 20
# iterations. Default 900s, because ITERATIONS is an argument and the whole
# point of a soak is that someone will raise it; the number encodes "a soak
# still running after fifteen minutes has stopped soaking and started
# leaking", not "1s plus slack". The cap is on the whole script: the work is a
# node test process that itself starts and stops servers.
# Override with DL_LEAK_BUDGET_S.
set -euo pipefail

. "$(cd "$(dirname "$0")/../../tools" && pwd)/run-capped.sh"
cap_self "${DL_LEAK_BUDGET_S:-900}" dl_leak_soak "$@"

cd "$(dirname "$0")/.."

PORT="${DL_LEAK_PORT:-17492}"
ITERATIONS="${DL_LEAK_ITERATIONS:-20}"
if [ -n "${DL_PERF_LOG:-}" ]; then
  LOG_PATH="$DL_PERF_LOG"
else
  LOG_PATH="$(mktemp "${TMPDIR:-/tmp}/dl-perf.XXXXXX.jsonl")"
fi

case "$ITERATIONS" in
  ''|*[!0-9]*)
    printf 'FAIL  DL_LEAK_ITERATIONS must be an integer >= 20\n' >&2
    exit 1
    ;;
esac

if [ "$ITERATIONS" -lt 20 ]; then
  printf 'FAIL  leak soak requires ITERATIONS >= 20, got %s\n' "$ITERATIONS" >&2
  exit 1
fi

DL_LEAK_PORT="$PORT" DL_LEAK_ITERATIONS="$ITERATIONS" DL_PERF_LOG="$LOG_PATH" \
  node --test --experimental-transform-types tests/8_leak_soak.test.ts
