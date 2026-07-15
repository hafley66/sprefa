#!/usr/bin/env bash
# Fail-closed cold/warm benchmark for an already-built release `dl`.
#
#   bench/run.sh [program.dl] [root]
#
# The root is selected by cwd, matching the CLI contract (there is no --root).
# Set BENCH_BUILD=1 to opt into a release build. Optional limits:
# BENCH_MAX_COLD_SECS, BENCH_MAX_WARM_SECS, BENCH_MAX_RSS_MB.
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
CALLER_DIR=$(pwd -P)

absolute_file() {
  local input=$1 candidate dir base
  case "$input" in
    /*) candidate=$input ;;
    *) candidate="$CALLER_DIR/$input" ;;
  esac
  dir=$(dirname -- "$candidate")
  base=$(basename -- "$candidate")
  [ -d "$dir" ] || { echo "directory does not exist: $dir" >&2; return 1; }
  dir=$(CDPATH= cd -- "$dir" && pwd -P)
  [ -f "$dir/$base" ] || { echo "file does not exist: $dir/$base" >&2; return 1; }
  printf '%s/%s\n' "$dir" "$base"
}

absolute_dir() {
  local input=$1 candidate
  case "$input" in
    /*) candidate=$input ;;
    *) candidate="$CALLER_DIR/$input" ;;
  esac
  [ -d "$candidate" ] || { echo "root does not exist: $candidate" >&2; return 1; }
  (CDPATH= cd -- "$candidate" && pwd -P)
}

PROG=$(absolute_file "${1:-$REPO_ROOT/bench/rust.dl}")
ROOT=$(absolute_dir "${2:-$REPO_ROOT}")

case "${BENCH_BUILD:-0}" in
  0) ;;
  1)
    (cd -- "$REPO_ROOT" && CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}" cargo build --release --bin dl)
    ;;
  *) echo "BENCH_BUILD must be 0 or 1" >&2; exit 2 ;;
esac

BIN=$(absolute_file "${BENCH_BIN:-$REPO_ROOT/target/release/dl}")
[ -x "$BIN" ] || { echo "benchmark binary is not executable: $BIN" >&2; exit 2; }

export DL_RAYON_THREADS="${DL_RAYON_THREADS:-2}"
case "$DL_RAYON_THREADS" in
  ''|*[!0-9]*|0) echo "DL_RAYON_THREADS must be a positive integer" >&2; exit 2 ;;
esac

is_number() {
  [[ $1 =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]]
}
for budget_name in BENCH_MAX_COLD_SECS BENCH_MAX_WARM_SECS BENCH_MAX_RSS_MB; do
  budget=${!budget_name:-}
  if [ -n "$budget" ] && ! is_number "$budget"; then
    echo "$budget_name must be a non-negative number, got: $budget" >&2
    exit 2
  fi
done

TIME_BIN=/usr/bin/time
[ -x "$TIME_BIN" ] || { echo "required timer not found: $TIME_BIN" >&2; exit 2; }
case "$(uname -s)" in
  Darwin)
    TIME_ARGS=(-l)
    RSS_DIVISOR=1048576 # BSD time reports bytes.
    TIME_STYLE=darwin
    ;;
  Linux)
    TIME_ARGS=(-v)
    RSS_DIVISOR=1024 # GNU time reports KiB.
    TIME_STYLE=gnu
    ;;
  *) echo "unsupported platform for peak-RSS measurement" >&2; exit 2 ;;
esac

WORKDIR=$(mktemp -d "${TMPDIR:-/tmp}/sprefa-bench.XXXXXX")
cleanup() { command rm -rf -- "$WORKDIR"; }
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
DB="$WORKDIR/bench.db"

LINUX_SIM=$(absolute_dir "$REPO_ROOT/bench/linux-sim")
PRINTK_PROG=$(absolute_file "$REPO_ROOT/bench/printk.dl")
if [ "$ROOT" = "$LINUX_SIM" ]; then
  [ "$PROG" = "$PRINTK_PROG" ] || {
    echo "linux-sim has a mandatory oracle and must use $PRINTK_PROG" >&2
    exit 2
  }
  command -v python3 >/dev/null 2>&1 || {
    echo "python3 is required for exact linux-sim result validation" >&2
    exit 2
  }
fi

parse_timing() {
  local timing=$1 elapsed raw_rss
  if [ "$TIME_STYLE" = darwin ]; then
    elapsed=$(awk '$2 == "real" { print $1; exit }' "$timing")
    raw_rss=$(awk '$2 == "maximum" && $3 == "resident" { print $1; exit }' "$timing")
  else
    elapsed=$(sed -n 's/^[[:space:]]*Elapsed (wall clock) time (h:mm:ss or m:ss):[[:space:]]*//p' "$timing" | head -n 1)
    raw_rss=$(sed -n 's/^[[:space:]]*Maximum resident set size (kbytes):[[:space:]]*//p' "$timing" | head -n 1)
    elapsed=$(awk -F: '
      NF == 3 { printf "%.6f", ($1 * 3600) + ($2 * 60) + $3; next }
      NF == 2 { printf "%.6f", ($1 * 60) + $2; next }
      NF == 1 { printf "%.6f", $1; next }
      { exit 1 }
    ' <<<"$elapsed")
  fi
  is_number "$elapsed" || { echo "could not parse wall time from $timing" >&2; return 1; }
  is_number "$raw_rss" || { echo "could not parse peak RSS from $timing" >&2; return 1; }
  RUN_WALL=$elapsed
  RUN_RSS_MIB=$(awk -v rss="$raw_rss" -v divisor="$RSS_DIVISOR" 'BEGIN { printf "%.3f", rss / divisor }')
}

validate_linux_sim() {
  local output=$1
  [ "$ROOT" = "$LINUX_SIM" ] || return 0
  python3 - "$output" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
lines = [line for line in path.read_text().splitlines() if line.strip()]
if len(lines) != 1:
    raise SystemExit(f"linux-sim oracle: expected one JSON line, got {len(lines)}")
try:
    actual = json.loads(lines[0])
except json.JSONDecodeError as error:
    raise SystemExit(f"linux-sim oracle: invalid JSON: {error}") from error
expected = {
    "query": "printk",
    "columns": ["path", "line"],
    "rows": [
        ["drivers/bar.c", 5],
        ["drivers/bar.c", 7],
        ["drivers/foo.c", 5],
        ["drivers/foo.c", 7],
    ],
    "count": 4,
}
if set(actual) != set(expected):
    raise SystemExit(f"linux-sim oracle: wrong JSON fields: {sorted(actual)}")
actual["rows"] = sorted(actual.get("rows", []))
expected["rows"] = sorted(expected["rows"])
if actual != expected:
    raise SystemExit(
        "linux-sim oracle mismatch\n"
        f"expected: {json.dumps(expected, sort_keys=True)}\n"
        f"actual:   {json.dumps(actual, sort_keys=True)}"
    )
PY
}

enforce_budget() {
  local name=$1 actual=$2 limit=${3:-}
  [ -n "$limit" ] || return 0
  awk -v actual="$actual" -v limit="$limit" 'BEGIN { exit !(actual <= limit) }' || {
    echo "$name budget exceeded: $actual > $limit" >&2
    return 1
  }
}

run_once() {
  local label=$1 stem=$2 output="$WORKDIR/$stem.jsonl"
  local stderr="$WORKDIR/$stem.stderr" timing="$WORKDIR/$stem.time"
  if ! (
    cd -- "$ROOT"
    "$TIME_BIN" "${TIME_ARGS[@]}" -o "$timing" \
      "$BIN" "$PROG" --db "$DB" --no-daemon --query-json \
      >"$output" 2>"$stderr"
  ); then
    echo "[$label] failed" >&2
    sed 's/^/  /' "$stderr" >&2
    return 1
  fi
  parse_timing "$timing"
  validate_linux_sim "$output"
  echo "[$label]"
  awk '/\[tick\]/ { print "  " $0 }' "$stderr"
  sed 's/^/  /' "$output"
  printf '  wall: %ss   peak RSS: %s MiB\n' "$RUN_WALL" "$RUN_RSS_MIB"
}

echo "bench: $PROG"
echo "root:  $ROOT"
echo "bin:   $BIN"
echo "rayon: $DL_RAYON_THREADS thread(s)"

run_once "COLD (fresh db, full extraction)" cold
COLD_WALL=$RUN_WALL
COLD_RSS=$RUN_RSS_MIB
enforce_budget cold_seconds "$COLD_WALL" "${BENCH_MAX_COLD_SECS:-}"
enforce_budget cold_rss_mib "$COLD_RSS" "${BENCH_MAX_RSS_MB:-}"

run_once "WARM (incremental, no edits)" warm
WARM_WALL=$RUN_WALL
WARM_RSS=$RUN_RSS_MIB
enforce_budget warm_seconds "$WARM_WALL" "${BENCH_MAX_WARM_SECS:-}"
enforce_budget warm_rss_mib "$WARM_RSS" "${BENCH_MAX_RSS_MB:-}"
