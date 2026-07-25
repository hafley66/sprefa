#!/bin/bash
# Lab runner: N hermetic repeats of cell x engine, one RESULT line per run.
# usage: lab_run.sh <label> [repeats]   (repeats default 3)
# cells + engines are fixed inside; edit here, not on the command line, so every
# experiment logs the identical matrix.
set -euo pipefail
BIN="$(cd "$(dirname "$0")/.." && pwd)/target/release/examples/perf_report"
LABEL="${1:-run}"
REPEATS="${2:-3}"
CELLS=("DAG60k 6 10000 0" "DAG960k 6 160000 0" "CYC960k 6 160000 7")
ENGINES=("sqlite-count" "sqlite-count-scc" "sqlite-dred-loop" "sqlite-dred-cte")
for cell in "${CELLS[@]}"; do
  read -r name layers width stride <<<"$cell"
  echo "# cell=$name oracle"
  "$BIN" oracle "$layers" "$width" "$stride" | sed "s/^/CELL=$name LABEL=$LABEL rep=0 /"
  for engine in "${ENGINES[@]}"; do
    for rep in $(seq 1 "$REPEATS"); do
      "$BIN" "$engine" "$layers" "$width" "$stride" | sed "s/^/CELL=$name LABEL=$LABEL rep=$rep /"
    done
  done
done
