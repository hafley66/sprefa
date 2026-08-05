# exec_shootout STANDINGS

Run command: `./target/release/harness --engines ref --scales 10000`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **-** | **0** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=79 cols=79 | 12324 | ref | 9979359 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **-** | **0** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **-** | **0** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
