#!/usr/bin/env bash
# Alternating base/main runs of the list ladder at 1M on one quiet box.
set -euo pipefail
probe=/Users/chrishafley/projects/sprefa-wt/sqlite-perf-main/plans/v8/probes/list_ladder/run.py
sed 's|ru_maxrss / 1048576|ru_maxrss / 1024|' "$probe" > ~/rcargo/run_linux.py
cd /Users/chrishafley/projects/sprefa-wt/sqlite-perf-main
uptime
for round in 1 2 3; do
  for build in bench-raw-int bench-sqlite-perf-main; do
    printf '%s\t' "$build"
    python3 ~/rcargo/run_linux.py ~/rcargo/$build/release/dl8 --ops 1048576 | tail -1
  done
done
uptime
