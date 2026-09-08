#!/usr/bin/env bash
set -uo pipefail

runtime="${PG_REACH_RUNTIME:-pglite}"
label="pglite-query"
if [[ "$runtime" == native ]]; then label="native-postgres-query"; fi
dir=$(cd "$(dirname "$0")" && pwd)
root="${PG_BENCH_RUN_ROOT:-${TMPDIR:-/tmp}}"
if [[ "$runtime" == native && "${PG_NATIVE_READY:-0}" != "1" ]]; then
  printf 'STATUS|%s|missing|%s|no process started|DL_MEMCAP_MB is unenforced for total PostgreSQL memory\n' \
    "$label" "${PG_NATIVE_REASON:-native PostgreSQL is unavailable}" >&2
  exit 0
fi
if [[ "$runtime" == pglite && ! -d "${PG_DEPENDENCY_DIR:-}/node_modules/@electric-sql/pglite" ]]; then
  printf 'STATUS|%s|missing|task-local PGlite package is unavailable|no process started|not applicable\n' \
    "$label" >&2
  exit 0
fi
case_dir=$(mktemp -d "$root/$label.XXXXXX")
trap 'rm -rf -- "$case_dir"' EXIT
budget_s="${PG_BENCH_BUDGET_S:-120}"
cap_mb="${DL_MEMCAP_MB:-4096}"
log="$case_dir/worker.log"

. "$dir/../../../tools/run-capped.sh"
run_capped "$budget_s" node --max-old-space-size="$cap_mb" \
  "$dir/1_postgres_reach.mjs" "$runtime" "$1" "$2" "$case_dir/data" \
  >"$log" 2>&1
status=$?
cat "$log" >&2
if [[ "$status" -eq 124 || "$status" -eq 142 ]]; then
  printf 'STATUS|%s|timeout|case exceeded %s seconds|unavailable|total memory unenforced\n' \
    "$label" "$budget_s" >&2
  exit 0
fi
if [[ "$status" -ne 0 && "$(grep -c '^STATUS|' "$log" || true)" -eq 0 ]]; then
  if grep -qi 'heap out of memory\|allocation failed' "$log"; then
    printf 'STATUS|%s|oom|worker reported an allocation failure|unavailable|Node old-space limit %s MiB; total memory unenforced\n' \
      "$label" "$cap_mb" >&2
  else
    printf 'STATUS|%s|error|worker exited with status %s|unavailable|total memory unenforced\n' \
      "$label" "$status" >&2
  fi
fi
exit 0
