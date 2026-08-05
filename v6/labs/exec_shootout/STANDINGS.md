# exec_shootout STANDINGS

Run command: `./harness/target/release/harness --engines interp/target/release/interp,rxgraph/target/release/rxgraph,mono/target/release/mono --scales 10000,100000,1000000 --measure-builds --work /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/92b4dd87-37e6-487b-9aee-eb00071bff32/scratchpad/ladder4`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | interp | 9996213 | 7269973 | 1 | 1375 | 1373648 | 3 |
|  |  |  | rxgraph | 9996213 | 56158500 | 0 | 178 | 121904 | 3 |
|  |  |  | mono | 9996213 | 68467212 | 0 | 146 | 111104 | 3 |
|  |  |  | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **68467212** | | | | |
| 100000 | segment_len=200 | 99898 | interp | 9989800 | 6736210 | 17 | 1483 | 1192336 | 3 |
|  |  |  | rxgraph | 9989800 | 42509787 | 6 | 235 | 169168 | 3 |
|  |  |  | mono | 9989800 | 47345024 | 11 | 211 | 168800 | 3 |
| **best (THE number)** | | | | **mono** | **47345024** | | | | |
| 1000000 | segment_len=20 | 999989 | interp | 9999890 | 4750542 | 290 | 2105 | 1874576 | 3 |
|  |  |  | rxgraph | 9999890 | 23529153 | 61 | 425 | 385776 | 3 |
|  |  |  | mono | 9999890 | 19417262 | 216 | 515 | 420304 | 3 |
| **best (THE number)** | | | | **rxgraph** | **23529153** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=45 cols=45 | 3960 | interp | 1069200 | 8100000 | 0 | 132 | 142976 | 3 |
|  |  |  | rxgraph | 1069200 | 53460000 | 0 | 20 | 16096 | 3 |
|  |  |  | mono | 1069200 | 62894118 | 0 | 17 | 14496 | 3 |
|  |  |  | ref | 1069200 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **62894118** | | | | |
| 100000 | rows=65 cols=65 | 8320 | interp | 4596800 | 7331419 | 0 | 627 | 625904 | 3 |
|  |  |  | rxgraph | 4596800 | 40679646 | 0 | 113 | 54976 | 3 |
|  |  |  | mono | 4596800 | 49427957 | 0 | 93 | 49936 | 3 |
| **best (THE number)** | | | | **mono** | **49427957** | | | | |
| 1000000 | rows=94 cols=94 | 17484 | interp | 19927389 | 6801157 | 2 | 2930 | 2404096 | 3 |
|  |  |  | rxgraph | 19927389 | 33890117 | 0 | 588 | 305232 | 3 |
|  |  |  | mono | 19927389 | 41429083 | 1 | 481 | 255856 | 3 |
| **best (THE number)** | | | | **mono** | **41429083** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | interp | 9951396 | 6346554 | 1 | 1568 | 1338032 | 3 |
|  |  |  | rxgraph | 9951396 | 37838008 | 0 | 263 | 162048 | 3 |
|  |  |  | mono | 9951396 | 47163014 | 0 | 211 | 124576 | 3 |
|  |  |  | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **47163014** | | | | |
| 100000 | layers=6 width=1250 fanout=16 | 100000 | interp | 10403068 | 3185263 | 10 | 3266 | 2508208 | 3 |
|  |  |  | rxgraph | 10403068 | 13688247 | 5 | 760 | 1076576 | 3 |
|  |  |  | mono | 10403068 | 22324180 | 8 | 466 | 256256 | 3 |
| **best (THE number)** | | | | **mono** | **22324180** | | | | |
| 1000000 | layers=4 width=15000 fanout=8 | 360000 | interp | 9815343 | 12192973 | 44 | 805 | 1572432 | 3 |
|  |  |  | rxgraph | 9815343 | 40062624 | 20 | 245 | 433552 | 3 |
|  |  |  | mono | 9815343 | 51659700 | 33 | 190 | 221200 | 3 |
| **best (THE number)** | | | | **mono** | **51659700** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|
| interp | 498016 | 0.3 |
| rxgraph | 479280 | 0.1 |
| mono | 473424 | 0.0 |


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
