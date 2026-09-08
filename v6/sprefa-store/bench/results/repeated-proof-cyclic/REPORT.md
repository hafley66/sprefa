# Repeated shared root-retraction comparison

The warmup run is retained and excluded from all summaries. Each measured run
uses fresh engine processes and a fresh task-local PostgreSQL cluster. Engine
order rotates by three positions per run. Tables report medians and full ranges,
with no confidence or significance claim. Only groups present in all measured
runs, with at least five samples, enter the median charts.

Deletion, maintenance or recomputation/materialization, and count are timed.
Exact input and result validation is outside the clocks. PostgreSQL includes
durable commit and client round trips. SQLite runtime stores are in memory;
specialized cascade stores use their existing on-disk configuration. RSS and
memory-cap scopes differ by adapter and are recorded in the raw reports.
OS available-memory samples and engine order are retained in each raw directory.
The sqlite-signed-delta-v2 name currently runs full recursive recomputation with
implicit zero-indegree roots and only the current deletion excluded. Its single
deletion measurements pass, but repeated-deletion sequence checks fail.

![Median retraction](retract_ms.png)
![Median RSS](rss_mb.png)

| Engine | Nodes | Samples | Median retract ms | Min–max ms | Median RSS MB |
|---|---:|---:|---:|---:|---:|
| differential-dataflow | 402 | 5/5 | 0.311 | 0.307–0.342 | 4.2 |
| differential-dataflow | 12002 | 5/5 | 2.593 | 2.591–2.668 | 8.7 |
| differential-dataflow | 160002 | 5/5 | 29.306 | 28.947–30.241 | 75.2 |
| native-postgres-query | 402 | 5/5 | 2.949 | 2.852–3.052 | 160.2 |
| native-postgres-query | 12002 | 5/5 | 19.669 | 19.022–20.082 | 220.7 |
| native-postgres-query | 160002 | 5/5 | 246.357 | 241.759–252.409 | 480.4 |
| pglite-query | 402 | 5/5 | 2.174 | 2.123–2.251 | 1068.1 |
| pglite-query | 12002 | 5/5 | 29.264 | 29.008–29.484 | 1048.9 |
| pglite-query | 160002 | 5/5 | 442.891 | 439.898–445.474 | 1359.7 |
| sprefa-engine-rs | 402 | 5/5 | 1.411 | 1.379–1.469 | 8.5 |
| sprefa-engine-rs | 12002 | 5/5 | 57.729 | 56.399–59.689 | 61.5 |
| sprefa-engine-rs | 160002 | 5/5 | 1056.267 | 1033.193–1078.669 | 513.4 |
| sqlite-count-scc | 402 | 5/5 | 0.790 | 0.780–0.920 | 6.2 |
| sqlite-count-scc | 12002 | 5/5 | 20.677 | 20.351–21.019 | 13.8 |
| sqlite-count-scc | 160002 | 5/5 | 307.445 | 305.918–308.156 | 119.1 |
| sqlite-dred-cte | 402 | 5/5 | 0.802 | 0.767–0.825 | 6.6 |
| sqlite-dred-cte | 12002 | 5/5 | 30.658 | 29.258–31.126 | 14.2 |
| sqlite-dred-cte | 160002 | 5/5 | 432.373 | 426.872–436.015 | 125.2 |
| sqlite-dred-loop | 402 | 5/5 | 0.976 | 0.934–1.005 | 6.6 |
| sqlite-dred-loop | 12002 | 5/5 | 21.371 | 20.745–21.440 | 14.2 |
| sqlite-dred-loop | 160002 | 5/5 | 308.212 | 306.632–310.747 | 119.4 |
| sqlite-signed-delta-v2 | 402 | 5/5 | 0.406 | 0.400–0.477 | 6.5 |
| sqlite-signed-delta-v2 | 12002 | 5/5 | 13.379 | 13.092–13.485 | 13.6 |
| sqlite-signed-delta-v2 | 160002 | 5/5 | 189.453 | 185.400–191.608 | 130.6 |
| swi-incr | 402 | 5/5 | 1.000 | 1.000–1.000 | 13.0 |
| swi-incr | 12002 | 5/5 | 59.000 | 56.000–63.000 | 47.3 |
| swi-incr | 160002 | 5/5 | 1163.000 | 1148.000–1185.000 | 601.9 |
| tsv2-runtime | 402 | 5/5 | 3.830 | 3.659–4.010 | 146.8 |
| tsv2-runtime | 12002 | 5/5 | 46.339 | 46.078–47.429 | 192.0 |
| tsv2-runtime | 160002 | 5/5 | 717.583 | 713.982–747.709 | 731.9 |

[Machine-readable ranges](repeat-ranges.csv), [median chart input](results.csv).

## Raw run status

| Phase | Iteration | Rotation | Exit | Receipts |
|---|---:|---:|---:|---|
| warmup | 0 | 0 | 0 | [raw report](warmup-0/REPORT.md) |
| measured | 1 | 3 | 0 | [raw report](measured-1/REPORT.md) |
| measured | 2 | 6 | 0 | [raw report](measured-2/REPORT.md) |
| measured | 3 | 9 | 0 | [raw report](measured-3/REPORT.md) |
| measured | 4 | 12 | 0 | [raw report](measured-4/REPORT.md) |
| measured | 5 | 15 | 0 | [raw report](measured-5/REPORT.md) |
