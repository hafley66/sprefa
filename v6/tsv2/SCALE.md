# tsv2 and v1 generated-program scale data

The full harness report is [bench/out/REPORT.md](../sprefa-store/bench/out/REPORT.md). Raw per-cell JSONL is [tsv2-results.jsonl](../sprefa-store/bench/out/tsv2-results.jsonl) and [v1-results.jsonl](../sprefa-store/bench/out/v1-results.jsonl); `bench/out/` is gitignored because the raw records are generated run output.

Machine: Apple M2 Pro, 16 GB RAM, Node v24.15.0.

Load caveat: the run used a shared laptop environment. Each engine used a fresh `:memory:` SQLite connection for every cell, with no state shared across cells. One warmup was discarded before each measured run. Both engines recomputed the Datalog result per tick. The harness used 100 arrivals per tick. s1 and s2 each used `ROWS` arrivals; s3 used `ROWS` arrivals on each EDB relation, with the right relation arriving after the left relation.

The table below is copied from the generated-program section of [bench/out/REPORT.md](../sprefa-store/bench/out/REPORT.md). Each matrix cell has adjacent rows for tsv2 and v1. Wall, tick, p95, max, and normalized values are milliseconds; RSS values are MB; final table rows are counts.

| engine | shape | rows per EDB rel | status | reason | total wall ms | mean tick ms | p95 tick ms | max tick ms | final table rows | ms per 1k arrivals | RSS MB |
|---|---|---:|---|---|---:|---:|---:|---:|---|---:|---:|
| tsv2-gen | s1 | 1000 | OK | - | 65.622375 | 5.959037909090909 | 7.046292 | 7.046292 | change=1000; head=100 | 65.622375 | 153.015625 |
| v1-gen | s1 | 1000 | N/A | v1 AST/evalProgramSql has no keyed-replace edge semantics | - | - | - | - | - | - | - |
| tsv2-gen | s1 | 10000 | OK | - | 1956.096542 | 19.366103544554452 | 32.469958 | 34.392542 | change=10000; head=100 | 195.6096542 | 374.3125 |
| v1-gen | s1 | 10000 | N/A | v1 AST/evalProgramSql has no keyed-replace edge semantics | - | - | - | - | - | - | - |
| tsv2-gen | s1 | 100000 | OK | - | 177093.0725 | 176.91593660439548 | 344.188625 | 389.457083 | change=100000; head=100 | 1770.9307250000002 | 568.3125 |
| v1-gen | s1 | 100000 | N/A | v1 AST/evalProgramSql has no keyed-replace edge semantics | - | - | - | - | - | - | - |
| tsv2-gen | s2 | 1000 | OK | - | 47.52 | 4.7461542 | 5.753292 | 5.753292 | a=100; a_link=100; b=100; b_link=100; c=1000 | 47.52 | 142.40625 |
| v1-gen | s2 | 1000 | OK | - | 4.776917 | 0.4776917 | 0.556959 | 0.556959 | c=1000; b_link=100; a_link=100; b=100; a=100 | 4.776917 | 130.390625 |
| tsv2-gen | s2 | 10000 | OK | - | 1946.503209 | 19.463674169999994 | 33.76175 | 39.573042 | a=100; a_link=100; b=100; b_link=100; c=10000 | 194.6503209 | 300.125 |
| v1-gen | s2 | 10000 | OK | - | 195.334416 | 1.95334416 | 2.055709 | 2.48025 | c=10000; b_link=100; a_link=100; b=100; a=100 | 19.5334416 | 139 |
| tsv2-gen | s2 | 100000 | OK | - | 183068.234875 | 183.06801158299987 | 347.108875 | 391.906708 | a=100; a_link=100; b=100; b_link=100; c=100000 | 1830.6823487499998 | 682.359375 |
| v1-gen | s2 | 100000 | OK | - | 17054.852792 | 17.054852792000002 | 17.704125 | 24.310917 | c=100000; b_link=100; a_link=100; b=100; a=100 | 170.54852792000003 | 227.296875 |
| tsv2-gen | s3 | 1000 | DNF | warmup worker exit status 1 | - | - | - | - | - | - | - |
| v1-gen | s3 | 1000 | OK | - | 2950.965291 | 147.54826455 | 150.054916 | 150.469166 | left=1000; right=1000; combined=1000000 | 1475.4826455 | 135.28125 |
| tsv2-gen | s3 | 10000 | DNF | warmup worker exit status 1 | - | - | - | - | - | - | - |
| v1-gen | s3 | 10000 | DNF | warmup worker exit status 1 | - | - | - | - | - | - | - |
| tsv2-gen | s3 | 100000 | DNF | warmup worker exit status 1 | - | - | - | - | - | - | - |
| v1-gen | s3 | 100000 | DNF | warmup worker exit status 1 | - | - | - | - | - | - | - |

DNF records:

- tsv2 s3/1k, s3/10k, and s3/100k: warmup worker exited with status 1 after V8 last-GC output under the 512 MB Node heap ceiling.
- v1 s3/10k: warmup reached the 600-second limit, with `/usr/bin/time -l` reporting `600.02 real` and abnormal termination.
- v1 s3/100k: warmup reached the 600-second limit, with `/usr/bin/time -l` reporting `600.28 real` and abnormal termination.

N/A records:

- v1 s1/1k, s1/10k, and s1/100k: the v1 AST/evalProgramSql seam has no keyed-replace edge semantics, so the keyed-replace shape was not rewritten as a different relation shape.

Oracle receipt: the generated s1/1k term was run through `v6/prolog/conformance/ticklog.pl` and the tsv2 worker log was compared with `cmp`. Both logs contain 11 lines and have SHA-256 `70c519e88c9f77f8467e000398600ccd4174b3f5bef9c64c232801f2bff3ca19`.

V5 yardstick, quoted for context only and marked not-directly-comparable because it is a different workload:

> | cold multi-repo scan | 42,739 files / 389 repos / 5.9s (~7,244 files/s) | grafana corpus, ~/orgs/grafana |
