# Retraction tick bench

## Definition

| field | value |
|---|---|
| ticks | K=100 cumulative graph updates |
| update | remove one existing edge, then add one absent forward edge |
| seed | SplitMix64, `0x6d65726375727901` |
| reset | each warmup or measured pass starts from the same base graph and seed |
| answer | the contract engine must emit the full `derived` count and checksum after every tick |
| timing boundary | child process spawn through exit; input rewrite is outside the timer |
| metric | median and nearest-rank p95 child wall time, plus sum of tick wall times |
| repetitions | 1 warmup pass, then 3 measured passes |

Forward means `source < target`. This keeps both supplied DAG families acyclic.
The edge count remains constant. The driver rejects duplicate base edges,
nonzero engine exits, malformed JSONL, and answer changes between measured
passes.

## Run

```bash
./run.sh \
  --engine ../mono/target/release/mono \
  --input <harness-work>/chain_10000.in \
  --ticks 100 --warmups 1 --passes 3 \
  --seed 0x6d65726375727901 \
  --work <temporary-work> \
  --label mono-chain-10000 \
  --output <temporary-results.tsv>
```

The same command accepts any executable satisfying `../CONTRACT.md`.

## Measurements

All measurement rows below use Apple M2 Pro, arm64, macOS 14.6.1 (23G93),
2026-08-14. Each case has 1 warmup pass and 3 measured passes. Each pass has
100 ticks. Times are child-process wall milliseconds.

| engine | family/scale | phase | pass | ticks | median ms | p95 ms | total ms |
|---|---|---|---|---|---|---|---|
| mono | chain 10000 | warmup | 1 | 100 | 55.118 | 202.723 | 7429.812 |
| mono | chain 10000 | measured | 1 | 100 | 55.627 | 201.999 | 7500.330 |
| mono | chain 10000 | measured | 2 | 100 | 55.308 | 204.261 | 7467.901 |
| mono | chain 10000 | measured | 3 | 100 | 55.280 | 203.599 | 7472.707 |
| mono | grid 10000 | warmup | 1 | 100 | 44.394 | 50.182 | 4258.948 |
| mono | grid 10000 | measured | 1 | 100 | 44.047 | 49.091 | 4236.814 |
| mono | grid 10000 | measured | 2 | 100 | 44.130 | 49.247 | 4251.480 |
| mono | grid 10000 | measured | 3 | 100 | 43.983 | 48.905 | 4237.210 |
| mercury-semi-naive | chain 10000 | warmup | 1 | 100 | 219.135 | 681.917 | 27976.412 |
| mercury-semi-naive | chain 10000 | measured | 1 | 100 | 212.303 | 687.448 | 27761.898 |
| mercury-semi-naive | chain 10000 | measured | 2 | 100 | 215.063 | 681.993 | 27534.208 |
| mercury-semi-naive | chain 10000 | measured | 3 | 100 | 214.565 | 677.519 | 27746.629 |
| mercury-semi-naive | grid 10000 | warmup | 1 | 100 | 148.447 | 160.867 | 14431.048 |
| mercury-semi-naive | grid 10000 | measured | 1 | 100 | 143.219 | 156.207 | 14009.156 |
| mercury-semi-naive | grid 10000 | measured | 2 | 100 | 143.742 | 154.052 | 13916.459 |
| mercury-semi-naive | grid 10000 | measured | 3 | 100 | 143.665 | 156.220 | 13909.625 |

Aggregate metrics combine the 300 measured ticks. Aggregate total is the sum
of child wall time across the 3 measured passes.

| engine | family/scale | measured ticks | median ms | p95 ms | aggregate total ms | machine | date | runs |
|---|---|---|---|---|---|---|---|---|
| mono | chain 10000 | 300 | 55.514 | 204.261 | 22440.938 | Apple M2 Pro | 2026-08-14 | 3 |
| mono | grid 10000 | 300 | 44.049 | 49.166 | 12725.504 | Apple M2 Pro | 2026-08-14 | 3 |
| mercury-semi-naive | chain 10000 | 300 | 214.253 | 683.519 | 83042.735 | Apple M2 Pro | 2026-08-14 | 3 |
| mercury-semi-naive | grid 10000 | 300 | 143.607 | 155.618 | 41835.240 | Apple M2 Pro | 2026-08-14 | 3 |

For all 400 tick positions per family, mono and mercury-semi-naive emitted
identical `(derived, checksum)` values. This includes the warmup and 3
measured passes.

## Gap statement

The measured from-scratch floor is 44.049 to 214.253ms at the median and
49.166 to 683.519ms at p95. Across 3 measured passes, 100 ticks consumed
12.726 to 83.043 seconds of engine process wall time per case. A dd-class
incremental entrant would update the changed dependency cone per tick. Its
delta-proportional number is not built yet; these four from-scratch rows are
the comparison floor for that future emitter output.
