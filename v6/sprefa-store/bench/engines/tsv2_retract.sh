#!/usr/bin/env bash
# tsv2 emitted-SQL retraction runner for the perf_report reach workload.
# Usage: tsv2_retract.sh <DAG|CYC> <layers> <width> <back-stride>
set -euo pipefail

engine_dir="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$engine_dir/../../../.." && pwd)"
scratch="$(mktemp -d)"
database="$scratch/tsv2-retract.sqlite"
stdout_file="$scratch/stdout"
stderr_file="$scratch/stderr"

set +e
/usr/bin/time -l node --max-old-space-size="${TSV2_HEAP_MB:-2048}" \
  --experimental-transform-types \
  "$root/v6/tsv2/scripts/2_p3-retract-bench.ts" \
  "$1" "$2" "$3" "$4" "$database" >"$stdout_file" 2>"$stderr_file"
worker_status=$?
set -e

rss_bytes="$(awk '/maximum resident set size/ {print $1}' "$stderr_file")"
csv_line="$(grep '^CSV,' "$stderr_file" | head -1)"
if [[ -z "$csv_line" ]]; then
  sed -n '1,40p' "$stderr_file" >&2
  rm -rf "$scratch"
  exit "$worker_status"
fi
if [[ -n "$rss_bytes" ]]; then
  rss_mb="$(awk -v bytes="$rss_bytes" 'BEGIN { printf "%.1f", bytes / 1048576 }')"
else
  rss_mb="$(sed -n 's/.*"process_rss_mb":\\([0-9.]*\\).*/\\1/p' "$stdout_file" | head -1)"
fi

cat "$stdout_file"
echo "${csv_line/RSS_FROM_TIME/$rss_mb}" >&2
rm -rf "$scratch"
