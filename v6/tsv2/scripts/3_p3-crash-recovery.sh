#!/usr/bin/env bash
# Kill the recursive-CTE cascade after its measured transaction starts, then
# reopen the same scratch database and verify SQLite rolled the tick back.
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
scratch="$(mktemp -d)"
database="$scratch/crash.sqlite"
marker="$scratch/measured"
stdout_file="$scratch/stdout"
stderr_file="$scratch/stderr"

P3_KEEP_DB=1 P3_MEASURE_MARKER="$marker" \
  node --max-old-space-size=1024 --experimental-transform-types \
  "$script_dir/2_p3-retract-bench.ts" \
  CYC 6 10000 7 "$database" >"$stdout_file" 2>"$stderr_file" &
worker_pid=$!

for _ in $(seq 1 200); do
  [[ -f "$marker" ]] && break
  kill -0 "$worker_pid" 2>/dev/null || {
    sed -n '1,40p' "$stderr_file" >&2
    rm -rf "$scratch"
    exit 1
  }
  sleep 0.01
done
[[ -f "$marker" ]] || {
  kill -9 "$worker_pid" 2>/dev/null || true
  wait "$worker_pid" 2>/dev/null || true
  rm -rf "$scratch"
  echo "P3_CRASH_FAIL measured marker was not reached" >&2
  exit 1
}

sleep 0.01
kill -9 "$worker_pid"
wait "$worker_pid" 2>/dev/null || true

receipt="$(sqlite3 "$database" 'SELECT count(*) || "," || sum(weight>0) || "," || (SELECT weight FROM cx_row WHERE key=0) FROM cx_row;')"
if [[ "$receipt" != "60002,60002,1" ]]; then
  echo "P3_CRASH_FAIL recovered=$receipt expected=60002,60002,1" >&2
  rm -rf "$scratch"
  exit 1
fi

echo '{"crash":"SIGKILL-mid-recursive-cte","recovered_rows":60002,"recovered_alive":60002,"root_weight":1,"result":"PASS"}'
rm -rf "$scratch"
