#!/usr/bin/env bash
# Bench the dl engine and report wall time + peak RSS.
#   bench/run.sh [prog.dl] [root]
# Defaults: bench/rust.dl over the parent dir (the sprefa repo).
# For the linux-checkout equivalent:  bench/run.sh bench/c.dl /path/to/linux
set -uo pipefail

PROG="${1:-bench/rust.dl}"
ROOT="${2:-..}"
cd "$(dirname "$0")/.."
cargo build --release -q

TIME=/usr/bin/time   # macOS BSD time; supports -l for max RSS
DB=/tmp/dlbench.db

run() {                       # run() <label>
  local err out; err=$(mktemp); out=$(mktemp)
  "$TIME" -l ./target/release/dl "$PROG" --root "$ROOT" --db "$DB" >"$out" 2>"$err" || true
  echo "[$1]"
  grep -E "\[tick\]" "$err" | sed 's/^/  /'
  grep -E "rows\)" "$out"   | sed 's/^/  /'
  local real maxrss
  real=$(grep -E "real" "$err" | awk '{print $1}' | head -1)
  maxrss=$(grep -E "maximum resident set size" "$err" | awk '{print $1}' | head -1)
  if [ -n "${maxrss:-}" ]; then
    awk -v r="$real" -v m="$maxrss" 'BEGIN{printf "  wall: %ss   peak RSS: %.1f MB\n", r, m/1048576}'
  else
    grep -E "real|resident|memory" "$err" | sed 's/^/  /'
  fi
  rm -f "$err" "$out"
}

echo "bench: $PROG   root: $ROOT"
rm -f "$DB"*
run "COLD (fresh db, full extraction)"
run "WARM (incremental, no edits)"
rm -f "$DB"*
