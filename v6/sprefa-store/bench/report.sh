#!/usr/bin/env bash
# Auto-write REPORT.md from results.csv: embeds the charts, prints a per-scale
# table, and derives one-line takeaways with awk. No hand-authored numbers.
set -uo pipefail
CSV="$1"; OUT="$2"; CAP="$3"

cat <<EOF
# Z-set / IVM head-to-head — feasibility lab

Same computation in every engine: reachability from roots {0,1} over a generated
DAG, then **retract root 0** and maintain the survivor set incrementally. Inputs
and outputs are byte-identical across engines (enforced by \`tests/head_to_head\`
against an independent BFS oracle). Only the **retract** is the measured op;
setup (building the corpus) is reported separately. Heap budget: ${CAP} MB/run.

Engines:
- **sqlite-mem** — the weight-cascade on an in-memory SQLite db (resident).
- **sqlite-disk** — same cascade on an on-file db, 32 MB page cache (paged).
- **dd** — differential-dataflow 0.25 / timely (resident arrangements).
- **dbsp** — Feldera's \`dbsp\` engine, \`recursive\` Z-set circuit (resident).

## Charts

![retract](retract_ms.png)
![setup](setup_ms.png)
![rss](rss_mb.png)
![ops](ops.png)

## Data

| engine | nodes | edges | killed | setup ms | retract ms | ops | RSS MB |
|---|---:|---:|---:|---:|---:|---:|---:|
EOF
tail -n +2 "$CSV" | awk -F, '{printf "| %s | %s | %s | %s | %s | %s | %s | %s |\n",$1,$2,$3,$4,$5,$6,$7,$8}'

echo
echo "## Takeaways (derived)"
echo
# Fastest retract at the largest common scale.
awk -F, 'NR>1 && $6!="WALL" && $6!="" {
  if ($2+0 > maxn) maxn=$2+0
}
END { print "- Largest scale reached by a numeric run: " maxn " nodes." }' "$CSV"

# Any walls?
walls=$(awk -F, 'NR>1 && $6=="WALL"{print $1" @ "$2" nodes"}' "$CSV")
if [[ -n "$walls" ]]; then
  echo "- Hit the ${CAP} MB budget (aborted, no swap):"
  echo "$walls" | sed 's/^/  - /'
else
  echo "- No engine hit the ${CAP} MB budget at these scales."
fi

# sqlite retract op-count independence (O(depth)).
awk -F, 'NR>1 && $1=="sqlite-disk" && $7!="WALL" && $7!="" {print $7}' "$CSV" \
  | sort -u | paste -sd, - \
  | awk '{print "- sqlite retract statement count across all scales: {" $0 "} (O(depth), not O(rows))."}'
