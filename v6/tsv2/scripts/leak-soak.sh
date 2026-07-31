#!/usr/bin/env bash
# leak-soak.sh — the served tsv2 engine's no-leak gate (receipt (c) of
# plans/2026-07-29-runtime-bridge-header.md). The assertions live in the test so
# the server process can inspect its own active resources; this only supplies
# the tracing log the statements-per-tick assertion reads.
#
# SABOTAGE RECEIPT: dropping `bumpActive(-1)` from 4_http.ts's SSE finalize
# fails at "timeout waiting for SSE teardown (iteration 0)"; merging each bind's
# timers twice fails at "timeout waiting for the interval timer to be the only
# one". The test header records the one sabotage that does NOT flip it.
#
# BUDGET (timeout-gun lane, 2026-07-31). Measured wall: 5s at the default 20
# iterations. Default 900s, because TSV2_LEAK_ITERATIONS is an argument and a
# soak exists to be raised; the number encodes "still running after fifteen
# minutes" rather than a multiple of five seconds. Whole-script cap: the work
# is a node test process that starts and swaps servers 20 times, so an
# orphaned generation is exactly what the process-group kill is for.
# Override with TSV2_LEAK_BUDGET_S.
set -euo pipefail

. "$(cd "$(dirname "$0")/../.." && pwd)/tools/run-capped.sh"
cap_self "${TSV2_LEAK_BUDGET_S:-900}" tsv2_leak_soak "$@"

cd "$(dirname "$0")/.."

PORT="${TSV2_LEAK_PORT:-17551}"
ITERATIONS="${TSV2_LEAK_ITERATIONS:-20}"
if [ -n "${DL_PERF_LOG:-}" ]; then
  LOG_PATH="$DL_PERF_LOG"
else
  LOG_PATH="$(mktemp "${TMPDIR:-/tmp}/tsv2-perf.XXXXXX.jsonl")"
fi

case "$ITERATIONS" in
  ''|*[!0-9]*)
    printf 'FAIL  TSV2_LEAK_ITERATIONS must be an integer >= 20\n' >&2
    exit 1
    ;;
esac

if [ "$ITERATIONS" -lt 20 ]; then
  printf 'FAIL  leak soak requires ITERATIONS >= 20, got %s\n' "$ITERATIONS" >&2
  exit 1
fi

TSV2_LEAK_PORT="$PORT" TSV2_LEAK_ITERATIONS="$ITERATIONS" DL_PERF_LOG="$LOG_PATH" \
  node --test --experimental-transform-types tests/serveLeak.test.ts
