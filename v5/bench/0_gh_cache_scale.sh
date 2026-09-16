#!/usr/bin/env bash
# Deterministic V5 gh-cache scale benchmark.
#
# Examples:
#   bench/0_gh_cache_scale.sh
#   ENDPOINTS=25,100,500 REPS=3 bench/0_gh_cache_scale.sh
#   OUT=bench/results/gh-cache.jsonl bench/0_gh_cache_scale.sh
set -euo pipefail

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/.." && pwd -P)
ENDPOINTS=${ENDPOINTS:-25,100,500}
REPS=${REPS:-1}
OUT=${OUT:-"$REPO_ROOT/target/gh-cache-scale.jsonl"}
PROFILE=${PROFILE:-release}
TIME_BIN=/usr/bin/time

case "$REPS" in
  ''|*[!0-9]*|0) echo "REPS must be a positive integer" >&2; exit 2 ;;
esac

IFS=, read -r -a SIZES <<<"$ENDPOINTS"
for size in "${SIZES[@]}"; do
  case "$size" in
    ''|*[!0-9]*|0) echo "ENDPOINTS must be comma-separated positive integers" >&2; exit 2 ;;
  esac
done

case "$(uname -s)" in
  Darwin) TIME_ARGS=(-l); TIME_STYLE=darwin ;;
  Linux) TIME_ARGS=(-v); TIME_STYLE=gnu ;;
  *) echo "unsupported platform for peak-RSS measurement" >&2; exit 2 ;;
esac

mkdir -p "$(dirname -- "$OUT")"
WORK=$(mktemp -d "${TMPDIR:-/tmp}/sprefa-gh-cache-scale.XXXXXX")
cleanup() { command rm -rf -- "$WORK"; }
trap cleanup EXIT

(
  cd -- "$REPO_ROOT"
  cargo test -p sprefa-dl --test it gh_cache::benchmark_cache_scale \
    --profile "$PROFILE" --no-run --message-format=json
) >"$WORK/build.jsonl"

TEST_BIN=$(python3 - "$WORK/build.jsonl" <<'PY'
import json
import pathlib
import sys

for line in pathlib.Path(sys.argv[1]).read_text().splitlines():
    try:
        row = json.loads(line)
    except json.JSONDecodeError:
        continue
    if row.get("reason") != "compiler-artifact":
        continue
    target = row.get("target", {})
    executable = row.get("executable")
    if executable and target.get("name") == "it":
        print(executable)
        break
else:
    raise SystemExit("could not locate the V5 integration-test executable")
PY
)

: >"$OUT"
for size in "${SIZES[@]}"; do
  for ((rep = 1; rep <= REPS; rep++)); do
    stdout="$WORK/$size-$rep.stdout"
    timing="$WORK/$size-$rep.time"
    SPREFA_GH_CACHE_ENDPOINTS="$size" \
      "$TIME_BIN" "${TIME_ARGS[@]}" -o "$timing" \
      "$TEST_BIN" --exact gh_cache::benchmark_cache_scale --ignored --nocapture \
      >"$stdout"

    python3 - "$stdout" "$timing" "$TIME_STYLE" "$rep" >>"$OUT" <<'PY'
import json
import pathlib
import re
import sys

stdout_path, timing_path, style, rep = sys.argv[1:]
stdout = pathlib.Path(stdout_path).read_text()
timing = pathlib.Path(timing_path).read_text()
match = re.search(r"^GH_CACHE_SCALE_JSON (.+)$", stdout, re.MULTILINE)
if not match:
    raise SystemExit(f"missing GH_CACHE_SCALE_JSON in {stdout_path}")
row = json.loads(match.group(1))
row["rep"] = int(rep)
if style == "darwin":
    wall = re.search(r"^\s*([0-9.]+)\s+real\b", timing, re.MULTILINE)
    rss = re.search(r"^\s*(\d+)\s+maximum resident set size\b", timing, re.MULTILINE)
    divisor = 1024 * 1024
else:
    elapsed = re.search(r"Elapsed \(wall clock\) time \(h:mm:ss or m:ss\):\s*(\S+)", timing)
    rss = re.search(r"Maximum resident set size \(kbytes\):\s*(\d+)", timing)
    if not elapsed:
        raise SystemExit(f"missing elapsed time in {timing_path}")
    parts = [float(part) for part in elapsed.group(1).split(":")]
    seconds = parts[0] if len(parts) == 1 else parts[-1] + 60 * parts[-2] + (3600 * parts[-3] if len(parts) == 3 else 0)
    wall = type("Match", (), {"group": lambda self, _: str(seconds)})()
    divisor = 1024
if not wall or not rss:
    raise SystemExit(f"missing wall/RSS measurement in {timing_path}")
row["process_wall_seconds"] = float(wall.group(1))
row["peak_rss_mib"] = round(int(rss.group(1)) / divisor, 3)
print(json.dumps(row, sort_keys=True, separators=(",", ":")))
PY
  done
done

python3 - "$OUT" <<'PY'
import json
import pathlib
import sys

rows = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
print("endpoints rep engine_s process_s rss_mib db_mib calls writes/tick")
for row in rows:
    db_mib = row["db_bytes"] / (1024 * 1024)
    print(
        f'{row["endpoints"]:9d} {row["rep"]:3d} '
        f'{row["engine_seconds"]:8.3f} {row["process_wall_seconds"]:9.3f} '
        f'{row["peak_rss_mib"]:7.1f} {db_mib:6.1f} '
        f'{row["calls"]:5d} {row["write_rows_per_tick"]:11.2f}'
    )
print(f"jsonl: {sys.argv[1]}")
PY
