#!/usr/bin/env bash
# memory-soak.sh -- NEW DIRECTIVE (user, 2026-07-29 late): prove node memory
# pressure stays CONSTANT under sustained assert/retract churn on one served
# tsv2 program. Entry point for scripts/memory-soak.ts (that file's header
# carries the churn-program design and the sabotage receipt).
#
# Short mode (default): ~100s, CI-friendly. Long mode (TSV2_SOAK_LONG=1):
# ~8h, meant for an overnight run, same cadences, more ticks.
#
# `--expose-gc` is passed so the sampler can force a collection right before
# reading process.memoryUsage() (scripts/memory-soak.ts's `sample`), cutting
# GC-timing noise out of the RSS/heapUsed comparison; the assertions do not
# depend on it (global.gc is optional there too).
set -uo pipefail
cd "$(dirname "$0")/.."

PORT="${TSV2_SOAK_PORT:-17571}"
if [ "${TSV2_SOAK_LONG:-0}" = "1" ]; then
  DURATION_S="${TSV2_SOAK_DURATION_S:-28800}"   # 8 hours
  SAMPLE_MS="${TSV2_SOAK_SAMPLE_MS:-10000}"
else
  DURATION_S="${TSV2_SOAK_DURATION_S:-100}"
  SAMPLE_MS="${TSV2_SOAK_SAMPLE_MS:-1000}"
fi

case "$DURATION_S" in
  ''|*[!0-9]*)
    printf 'FAIL  TSV2_SOAK_DURATION_S must be a positive integer, got %s\n' "$DURATION_S" >&2
    exit 1
    ;;
esac

WORK="$(mktemp -d "${TMPDIR:-/tmp}/tsv2-memsoak.XXXXXX")"
if [ -n "${DL_PERF_LOG:-}" ]; then
  LOG_PATH="$DL_PERF_LOG"
else
  LOG_PATH="$WORK/perf.jsonl"
fi
if [ -n "${TSV2_SOAK_RECEIPT:-}" ]; then
  RECEIPT_PATH="$TSV2_SOAK_RECEIPT"
else
  RECEIPT_PATH="$WORK/receipt.jsonl"
fi

TSV2_SOAK_PORT="$PORT" \
  TSV2_SOAK_DURATION_S="$DURATION_S" \
  TSV2_SOAK_ARRIVAL_MS="${TSV2_SOAK_ARRIVAL_MS:-40}" \
  TSV2_SOAK_SAMPLE_MS="$SAMPLE_MS" \
  TSV2_SOAK_KEYS="${TSV2_SOAK_KEYS:-25}" \
  TSV2_SOAK_RETENTION_CAP="${TSV2_SOAK_RETENTION_CAP:-200}" \
  TSV2_SOAK_TOLERANCE="${TSV2_SOAK_TOLERANCE:-0.10}" \
  TSV2_SOAK_SABOTAGE="${TSV2_SOAK_SABOTAGE:-}" \
  DL_PERF_LOG="$LOG_PATH" \
  TSV2_SOAK_RECEIPT="$RECEIPT_PATH" \
  node --expose-gc --experimental-transform-types scripts/memory-soak.ts
status=$?

printf 'perf log: %s\n' "$LOG_PATH"
printf 'receipt : %s\n' "$RECEIPT_PATH"
printf '(work dir kept for autopsy: %s)\n' "$WORK"
exit "$status"
