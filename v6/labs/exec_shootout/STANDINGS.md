# exec_shootout STANDINGS

Run command: `./harness/target/release/harness --engines interp/target/release/interp,rxgraph/target/release/rxgraph,mono/target/release/mono --scales 10000,100000,1000000 --measure-builds --work /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/92b4dd87-37e6-487b-9aee-eb00071bff32/scratchpad/ladder3`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | interp | 9996213 | 6424301 | 1 | 1556 | 1443376 | 3 |
|  |  |  | rxgraph | 9996213 | 53455684 | 0 | 187 | 131552 | 3 |
|  |  |  | mono | 9996213 | 66200086 | 0 | 151 | 111744 | 3 |
|  |  |  | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **66200086** | | | | |
| 100000 | segment_len=200 | 99898 | interp | 9989800 | 5125603 | 18 | 1949 | 1586304 | 3 |
|  |  |  | rxgraph | 9989800 | 40941803 | 5 | 244 | 175632 | 3 |
|  |  |  | mono | 9989800 | 44399111 | 11 | 225 | 177872 | 3 |
| **best (THE number)** | | | | **mono** | **44399111** | | | | |
| 1000000 | segment_len=20 | 999989 | interp | 9999890 | 2956798 | 271 | 3382 | 2265728 | 3 |
|  |  |  | rxgraph | 9999890 | 22271470 | 64 | 449 | 462576 | 3 |
|  |  |  | mono | 9999890 | 19959860 | 190 | 501 | 429504 | 3 |
| **best (THE number)** | | | | **rxgraph** | **22271470** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=45 cols=45 | 3960 | interp | 1069200 | 6006742 | 0 | 178 | 235392 | 3 |
|  |  |  | rxgraph | 1069200 | 48600000 | 0 | 22 | 18880 | 3 |
|  |  |  | mono | 1069200 | 59400000 | 0 | 18 | 16816 | 3 |
|  |  |  | ref | 1069200 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **59400000** | | | | |
| 100000 | rows=65 cols=65 | 8320 | interp | 4596800 | 5668064 | 0 | 811 | 884608 | 3 |
|  |  |  | rxgraph | 4596800 | 38628571 | 0 | 119 | 72336 | 3 |
|  |  |  | mono | 4596800 | 47389691 | 0 | 97 | 52448 | 3 |
| **best (THE number)** | | | | **mono** | **47389691** | | | | |
| 1000000 | rows=94 cols=94 | 17484 | interp | 19927389 | 4741230 | 2 | 4203 | 2593280 | 3 |
|  |  |  | rxgraph | 19927389 | 31381715 | 0 | 635 | 351648 | 3 |
|  |  |  | mono | 19927389 | 38248347 | 1 | 521 | 297648 | 3 |
| **best (THE number)** | | | | **mono** | **38248347** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | interp | 9951396 | 5650991 | 1 | 1761 | 1494256 | 3 |
|  |  |  | rxgraph | 9951396 | 37552438 | 0 | 265 | 157568 | 3 |
|  |  |  | mono | 9951396 | 44826108 | 0 | 222 | 131872 | 3 |
|  |  |  | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **44826108** | | | | |
| 100000 | layers=6 width=1250 fanout=16 | 100000 | interp | 10403068 | 2383292 | 9 | 4365 | 3049440 | 3 |
|  |  |  | rxgraph | 10403068 | 14270326 | 5 | 729 | 1062992 | 3 |
|  |  |  | mono | 10403068 | 21628000 | 8 | 481 | 248816 | 3 |
| **best (THE number)** | | | | **mono** | **21628000** | | | | |
| 1000000 | layers=4 width=15000 fanout=8 | 360000 | interp | 9815343 | 6119291 | 62 | 1604 | 1788512 | 3 |
|  |  |  | rxgraph | 9815343 | 36899786 | 21 | 266 | 456880 | 3 |
|  |  |  | mono | 9815343 | 47647296 | 36 | 206 | 230288 | 3 |
| **best (THE number)** | | | | **mono** | **47647296** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|
| interp | 792192 | 0.3 |
| rxgraph | 479280 | 0.1 |
| mono | 473424 | 0.0 |


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
