# tsv2 generated-program scale data

The full harness report is [bench/out/REPORT.md](../sprefa-store/bench/out/REPORT.md). Raw per-cell JSONL is [bench/out/tsv2-results.jsonl](../sprefa-store/bench/out/tsv2-results.jsonl); `bench/out/` is gitignored because the raw records are generated run output.

Machine: Apple M2 Pro, 16 GB RAM, Node v24.15.0.

Load caveat: the run used a shared laptop environment. The tsv2 runner used a fresh `:memory:` SQLite connection for every cell, with no state shared across cells. One warmup was discarded before each measured run. The harness used 100 arrivals per tick. s1 and s2 each used `ROWS` arrivals; s3 used `ROWS` arrivals on each EDB relation, with the right relation arriving after the left relation.

The table below is copied from the tsv2 section of `bench/out/REPORT.md`. Wall, tick, p95, max, and normalized values are milliseconds; RSS values are MB; final table rows are counts.

| shape | rows per EDB rel | status | total wall ms | mean tick ms | p95 tick ms | max tick ms | final table rows | ms per 1k arrivals |
|---|---:|---|---:|---:|---:|---:|---|---:|
| s1 | 1000 | OK | 65.107375 | 5.911594727272727 | 6.931375 | 6.931375 | change=1000; head=100 | 65.107375 |
| s1 | 10000 | OK | 2040.495042 | 20.201310237623762 | 34.502292 | 35.962375 | change=10000; head=100 | 204.0495042 |
| s1 | 100000 | OK | 185318.461375 | 185.13311584315665 | 361.306916 | 459.913625 | change=100000; head=100 | 1853.1846137500002 |
| s2 | 1000 | OK | 48.401792 | 4.8302209 | 6.356 | 6.356 | a=100; a_link=100; b=100; b_link=100; c=1000 | 48.401792 |
| s2 | 10000 | OK | 1896.851125 | 18.96707625 | 32.648541 | 36.223542 | a=100; a_link=100; b=100; b_link=100; c=10000 | 189.6851125 |
| s2 | 100000 | OK | 190267.591708 | 190.267307333 | 377.886292 | 415.594709 | a=100; a_link=100; b=100; b_link=100; c=100000 | 1902.6759170799999 |
| s3 | 1000 | DNF | DNF | DNF | DNF | DNF | DNF | DNF |
| s3 | 10000 | DNF | DNF | DNF | DNF | DNF | DNF | DNF |
| s3 | 100000 | DNF | DNF | DNF | DNF | DNF | DNF | DNF |

DNF records: s3 at 1k, 10k, and 100k. Each discarded warmup worker exited with status 1 after V8 last-GC output under the 512 MB Node heap ceiling; no measured number was recorded for those cells. The runner continued to the next cell.

Oracle receipt: the generated s1/1k term was run through `v6/prolog/conformance/ticklog.pl` and the tsv2 worker log was compared with `cmp`. Both logs contain 11 lines and have SHA-256 `70c519e88c9f77f8467e000398600ccd4174b3f5bef9c64c232801f2bff3ca19`.

V5 yardstick, quoted for context only and NOT directly comparable because it is a different workload:

> | cold multi-repo scan | 42,739 files / 389 repos / 5.9s (~7,244 files/s) | grafana corpus, ~/orgs/grafana |
