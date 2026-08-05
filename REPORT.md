# rxrace: standalone mono-vs-rxgraph same-input receipts

Chain@10k case, tuned per harness `tuner::tune(Family::Chain, 10_000)`:
segment_len=2582, edges=7743, nodes=7746. Input generated once via the harness
binary (`harness --engines ref --scales 10000 --work ...`), which wrote
`chain_10000.in`; both engines read that identical file outside the harness.
Seed for generation: `10000 ^ 0x5eed_cafe`, per CONTRACT.

Each engine binary run 3x with `/usr/bin/time -l`. Peak RSS column is the
engine's own reported `peak_rss_kb` (`getrusage ru_maxrss / 1024`); it matches
the `maximum resident set size` from `/usr/bin/time`.

| engine | run# | load ms | fixpoint ms | derived | checksum | wall s | peak RSS kB |
|---|---|---|---|---|---|---|---|
| mono | 1 | 1 | 9907 | 9996213 | df09b2f409f8b9a8 | 9.98 | 300432 |
| mono | 2 | 1 | 9949 | 9996213 | df09b2f409f8b9a8 | 10.02 | 300320 |
| mono | 3 | 0 | 10540 | 9996213 | df09b2f409f8b9a8 | 10.61 | 300208 |
| rxgraph | 1 | 0 | 323 | 9996213 | df09b2f409f8b9a8 | 0.33 | 301104 |
| rxgraph | 2 | 0 | 321 | 9996213 | df09b2f409f8b9a8 | 0.32 | 300624 |
| rxgraph | 3 | 0 | 324 | 9996213 | df09b2f409f8b9a8 | 0.33 | 300624 |

The two engines report identical derived counts (9996213) and identical
checksums (df09b2f409f8b9a8) on every run, so the same-input results match.

