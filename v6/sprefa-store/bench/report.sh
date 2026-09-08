#!/usr/bin/env bash
# Auto-write REPORT.md from results.csv: embeds the charts, prints a per-scale
# table, and derives one-line takeaways with awk. No hand-authored numbers.
set -uo pipefail
CSV="$1"; OUT="$2"; CAP="$3"; STATUS_TSV="${4:-}"

cat <<EOF
# Z-set / IVM head-to-head: feasibility lab

Same computation in every engine: reachability from roots {0,1} over a generated
DAG, then **retract root 0** and recount the survivor set. The PostgreSQL-family
numeric arms compare complete ordered results with an independent BFS oracle
before emitting CSV. Only the root retraction and survivor recount are the
measured operation; setup is reported separately. Requested memory budget:
${CAP} MB/run. Enforcement and accounting scope are adapter-specific and
recorded below.

Ordinary PostgreSQL and PGlite arms execute the recursive query from scratch
after the root deletion. Their timed phase ends after query materialization and
\`count(*)\`. Ordered full-result transfer, checksum, and exact validation are
recorded in the status receipt and excluded from \`setup_ms\` and
\`retract_ms\`. Their rows are full recomputation measurements.

The \`differential-dataflow\` arm is the native Differential Dataflow 0.25
library over timely 0.31. Its setup and retract phases end after fixed-point
convergence and counting. Complete ordered
initial and survivor sets are checked against an independent BFS outside the
timed phases.

## Charts

![retract](retract_ms.png)
![setup](setup_ms.png)
![rss](rss_mb.png)
![ops](ops.png)

## Data

| engine | nodes | edges | killed | setup ms | retract ms | ops | RSS MB | host peak MB | SQLite high-water MB | db MB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
EOF
tail -n +2 "$CSV" | awk -F, '{printf "| %s | %s | %s | %s | %s | %s | %s | %s | %s | %s | %s |\n",$1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11}'

if status_path="${STATUS_TSV:-}"; [[ -n "$status_path" && -s "$status_path" ]]; then
  echo
  echo "## Adapter status and measurement scope"
  echo
  echo "| engine | scale | status | semantics or reason | memory scope | memory-limit scope |"
  echo "|---|---|---|---|---|---|"
  tail -n +2 "$status_path" | awk -F '\t' '{printf "| %s | %s | %s | %s | %s | %s |\n",$1,$2,$3,$4,$5,$6}'
fi

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
  echo "- Recorded a WALL row under the requested ${CAP} MB budget or an unclassified engine failure:"
  echo "$walls" | sed 's/^/  - /'
else
  echo "- No numeric arm emitted a WALL row at these scales."
fi

# swi-sqlite retract op-count independence (O(depth)).
sqlite_ops=$(awk -F, 'NR>1 && $1=="swi-sqlite" && $7!="WALL" && $7!="" {print $7}' "$CSV" \
  | sort -u | paste -sd, -)
if [[ -n "$sqlite_ops" ]]; then
  echo "- swi-sqlite retract statement count across all scales: {$sqlite_ops} (O(depth), not O(rows))."
fi

if [[ -s "$OUT/tsv2-results.jsonl" || -s "$OUT/v1-results.jsonl" ]]; then
  echo
  echo "## Generated-program scale data: tsv2 and v1"
  echo
  echo "Each row is a fresh in-memory SQLite cell. Both engines recompute the Datalog result per tick; one warmup is discarded."
  echo
  echo "| engine | shape | rows per EDB rel | status | reason | total wall ms | mean tick ms | p95 tick ms | max tick ms | final table rows | ms per 1k arrivals | RSS MB |"
  echo "|---|---|---:|---|---|---:|---:|---:|---:|---|---:|---:|"
  node -e '
    const fs = require("fs");
    const rows = process.argv.slice(1).flatMap((path) => {
      if (!fs.existsSync(path)) return [];
      return fs.readFileSync(path, "utf8").split("\n").filter(Boolean).map(JSON.parse);
    });
    const order = { s1: 1, s2: 2, s3: 3 };
    rows.sort((a, b) => order[a.shape] - order[b.shape] || a.rows - b.rows || a.engine.localeCompare(b.engine));
    for (const r of rows) {
      const sizes = r.final_table_sizes ? Object.entries(r.final_table_sizes).map(([k,v]) => `${k}=${v}`).join("; ") : "-";
      const reason = r.reason ?? r.observed_failure ?? "-";
      console.log(`| ${r.engine} | ${r.shape} | ${r.rows} | ${r.status} | ${reason} | ${r.total_wall_ms ?? "-"} | ${r.mean_tick_ms ?? "-"} | ${r.p95_tick_ms ?? "-"} | ${r.max_tick_ms ?? "-"} | ${sizes} | ${r.ms_per_1k_arrivals ?? "-"} | ${r.worker_rss_mb ?? "-"} |`);
    }
  ' "$OUT/tsv2-results.jsonl" "$OUT/v1-results.jsonl"
fi
