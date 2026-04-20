#!/usr/bin/env bash
# run_probe.sh — harness for throughput_probe. Per-invocation RSS via
# /usr/bin/time -l (macOS). Shuffles (variant × pattern × trial) schedule.
# Emits TSV + summary.
#
# Usage: examples/run_probe.sh [ROOT] [MAX_FILES] [TRIALS]
#   ROOT       corpus root (default: tests/smoke/.fixtures/swc)
#   MAX_FILES  enumerate cap (default: 2000; blank = full)
#   TRIALS     per (variant, pattern) (default: 10)

set -euo pipefail

ROOT="${1:-tests/smoke/.fixtures/swc}"
# MAX_FILES: pass "all" or "0" to run uncapped. Default 2000.
MAX_FILES="${2:-2000}"
if [[ "$MAX_FILES" == "all" || "$MAX_FILES" == "0" ]]; then MAX_FILES=""; fi
TRIALS="${3:-10}"

BIN="target/release/examples/throughput_probe"
TSV="target/probe_results.tsv"
mkdir -p target

# Build once.
cargo build --release --example throughput_probe >&2

VARIANTS=(
  A
  Cdam:16 Cdam:256 Cdam:1024
  Cbatch:256:8 Cbatch:256:64
  Dtokio:32 Dtokio:128
  Dbatch:32:8 Dbatch:32:64
  Efat
)
PATTERNS=(baseline medium heavy nomatch)

MAX_FLAG=()
if [[ -n "$MAX_FILES" ]]; then MAX_FLAG=(--max "$MAX_FILES"); fi

echo -e "variant\tpattern\ttrial\tfiles\tbytes\tmatches\twall_ms\tmax_rss_bytes" > "$TSV"

# Build schedule, shuffle via awk + sort -R (deterministic-free).
SCHEDULE=$(
  for v in "${VARIANTS[@]}"; do
    for p in "${PATTERNS[@]}"; do
      for t in $(seq 1 "$TRIALS"); do
        echo -e "$v\t$p\t$t"
      done
    done
  done | awk 'BEGIN{srand()} {print rand()"\t"$0}' | sort -k1,1 | cut -f2-
)

TOTAL=$(echo "$SCHEDULE" | wc -l | tr -d ' ')
echo "scheduled $TOTAL runs ($((${#VARIANTS[@]} * ${#PATTERNS[@]})) cells × $TRIALS trials)" >&2
echo >&2

i=0
while IFS=$'\t' read -r V P T; do
  i=$((i + 1))
  printf "[%d/%d] %s / %s trial %s ... " "$i" "$TOTAL" "$V" "$P" "$T" >&2

  # /usr/bin/time -l on macOS reports "maximum resident set size" in bytes
  # (BSD convention; some older macOS show it in pages — we probe both).
  OUT=$(mktemp)
  ERR=$(mktemp)
  /usr/bin/time -l "$BIN" --variant "$V" --pattern "$P" --root "$ROOT" "${MAX_FLAG[@]}" \
    > "$OUT" 2> "$ERR" || { echo "FAIL" >&2; cat "$ERR" >&2; rm -f "$OUT" "$ERR"; continue; }

  JSON=$(cat "$OUT")
  RSS=$(awk '/maximum resident set size/ {print $1}' "$ERR")
  RSS="${RSS:-0}"

  FILES=$(echo "$JSON"   | sed -E 's/.*"files":([0-9]+).*/\1/')
  BYTES=$(echo "$JSON"   | sed -E 's/.*"bytes":([0-9]+).*/\1/')
  MATCHES=$(echo "$JSON" | sed -E 's/.*"matches":([0-9]+).*/\1/')
  WALL=$(echo "$JSON"    | sed -E 's/.*"wall_ms":([0-9.]+)\}.*/\1/')

  echo -e "$V\t$P\t$T\t$FILES\t$BYTES\t$MATCHES\t$WALL\t$RSS" >> "$TSV"
  printf "wall=%sms rss=%s\n" "$WALL" "$RSS" >&2

  rm -f "$OUT" "$ERR"
done <<< "$SCHEDULE"

echo >&2
echo "results: $TSV" >&2
echo >&2
exec "$(dirname "$0")/agg_probe.sh" "$TSV"
