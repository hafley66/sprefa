#!/usr/bin/env bash
# Feasibility-lab harness for the Z-set/IVM head-to-head. Runs every engine over
# a scale sweep, collects the `CSV,...` line each example prints on stderr, then
# renders charts (gnuplot) and an auto-written REPORT.md. No plotting library,
# no bespoke chart code: the engines emit CSV, gnuplot draws, awk writes prose.
#
# Usage:  bench/run.sh            # default sweep
#         SCALES="4x500 8x20000" bench/run.sh
#         CAP=4096 bench/run.sh   # heap budget (MB) per run
set -uo pipefail
cd "$(dirname "$0")/.."

OUT=bench/out
mkdir -p "$OUT"
CSV="$OUT/results.csv"
: > "$OUT/tsv2-results.jsonl"
CAP="${CAP:-4096}"
# Engines: label|binary|extra-env. sqlite runs twice (mem vs disk).
ENGINES=(
  "sqlite-mem|sqlite_reach|DL_SQLITE_MODE=mem"
  "sqlite-disk|sqlite_reach|DL_SQLITE_MODE=disk"
  "dd|dd_reach|"
  "dbsp|dbsp_reach|"
  "swi-incr|bench/engines/swi_incr.sh|"
  "swipl-pure|bench/engines/swipl_pure.sh|"
  "swi-sqlite|bench/engines/swi_sqlite.sh|"
  "swi-ts|bench/engines/swi_ts.sh|"
  "swi-emit|bench/engines/swi_emit.sh|"
  "tsv2-gen|bench/engines/tsv2_gen.sh|"
)
# Scale sweep as "layers x width". Kept medium so a laptop survives.
SCALES="${SCALES:-2x200 6x2000 8x20000 10x50000 14x80000}"

echo "engine,nodes,edges,killed,setup_ms,retract_ms,ops,rss_mb" > "$CSV"

for spec in "${ENGINES[@]}"; do
  IFS='|' read -r label bin env <<< "$spec"
  # entries with a slash are wrapper scripts (prolog contenders), used as-is
  if [[ "$bin" == */* ]]; then binpath="$bin"; else binpath="target/release/examples/$bin"; fi
  if [[ ! -x "$binpath" ]]; then
    echo "SKIP $label ($binpath not built)"; continue
  fi
  scales="$SCALES"
  if [[ "$label" == "tsv2-gen" ]]; then
    scales="${TSV2_SCALES:-1x1000 1x10000 1x100000 2x1000 2x10000 2x100000 3x1000 3x10000 3x100000}"
  fi
  for s in $scales; do
    layers="${s%x*}"; width="${s#*x}"
    cell_log=$(mktemp)
    env $env DL_MEMCAP_MB="$CAP" "$binpath" "$layers" "$width" >/dev/null 2>"$cell_log"
    status=$?
    line=$(grep '^CSV,' "$cell_log" | head -1 | cut -d, -f2-)
    if [[ "$label" == "tsv2-gen" && "$status" -ne 0 && \
          "$(grep -c '^TSV2_ORACLE_DIFF' "$cell_log")" -gt 0 ]]; then
      cat "$cell_log"
      rm -f "$cell_log"
      exit 1
    fi
    cat "$cell_log"
    rm -f "$cell_log"
    if [[ -n "$line" ]]; then
      echo "$line" >> "$CSV"
      echo "OK   $label $s -> $line"
    else
      # non-empty means it aborted/OOM'd at the cap: record as a wall hit.
      nodes=$(( 2 + layers * width ))
      echo "$label,$nodes,,,,,WALL," >> "$CSV"
      echo "WALL $label $s (hit the ${CAP}MB budget or failed)"
    fi
  done
done

echo "== wrote $CSV =="
bench/chart.sh "$CSV" "$OUT"
bench/report.sh "$CSV" "$OUT" "$CAP" > "$OUT/REPORT.md"
echo "== wrote $OUT/REPORT.md and PNG charts in $OUT/ =="
