#!/usr/bin/env bash
# leak-soak.sh — served tsv2 engine leak gate. Assertions live in the test so
# the server process can inspect its own active resources; this only supplies
# the tracing log the statements-per-tick assertion reads.
#
# Whole-script budget; override with TSV2_LEAK_BUDGET_S.
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

# scratchStoreClose's 335-round soak is gated on DL_PERF_LOG for the same reason
# serveLeak's receipt (c) is: 335 connections back to back saturate the machine,
# and in the default battery that wedged a sibling worker's spawned compile.
TSV2_LEAK_PORT="$PORT" TSV2_LEAK_ITERATIONS="$ITERATIONS" DL_PERF_LOG="$LOG_PATH" \
  node --test --experimental-transform-types tests/serveLeak.test.ts tests/scratchStoreClose.test.ts
