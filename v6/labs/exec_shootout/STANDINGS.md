# exec_shootout STANDINGS

Run command: `./harness/target/release/harness --engines interp/target/release/interp,rxgraph/target/release/rxgraph,mono/target/release/mono --scales 10000,100000,1000000 --measure-builds --work /Users/chrishafley/.claude/jobs/eae95965/tmp/shootout`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | interp | 9996213 | 7089513 | 1 | 1410 | 1253712 | 3 |
|  |  |  | rxgraph | 9996213 | 55534517 | 0 | 180 | 120896 | 3 |
|  |  |  | mono | 9996213 | 68001449 | 0 | 147 | 107904 | 3 |
|  |  |  | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **68001449** | | | | |
| 100000 | segment_len=200 | 99898 | interp | 9989800 | 5942772 | 17 | 1681 | 1150896 | 3 |
|  |  |  | rxgraph | 9989800 | 39175686 | 5 | 255 | 196896 | 3 |
|  |  |  | mono | 9989800 | 42509787 | 14 | 235 | 182640 | 3 |
| **best (THE number)** | | | | **mono** | **42509787** | | | | |
| 1000000 | segment_len=20 | 999989 | interp | 9999890 | 3852038 | 303 | 2596 | 1370752 | 3 |
|  |  |  | rxgraph | 9999890 | 20746660 | 62 | 482 | 470832 | 3 |
|  |  |  | mono | 9999890 | 15872841 | 238 | 630 | 339568 | 3 |
| **best (THE number)** | | | | **rxgraph** | **20746660** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=45 cols=45 | 3960 | interp | 1069200 | 7920000 | 0 | 135 | 159056 | 3 |
|  |  |  | rxgraph | 1069200 | 53460000 | 0 | 20 | 16096 | 3 |
|  |  |  | mono | 1069200 | 62894118 | 0 | 17 | 15872 | 3 |
|  |  |  | ref | 1069200 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **62894118** | | | | |
| 100000 | rows=65 cols=65 | 8320 | interp | 4596800 | 7028746 | 0 | 654 | 743280 | 3 |
|  |  |  | rxgraph | 4596800 | 40322807 | 0 | 114 | 80592 | 3 |
|  |  |  | mono | 4596800 | 48387368 | 0 | 95 | 49936 | 3 |
| **best (THE number)** | | | | **mono** | **48387368** | | | | |
| 1000000 | rows=94 cols=94 | 17484 | interp | 19927389 | 6495238 | 1 | 3068 | 2243024 | 3 |
|  |  |  | rxgraph | 19927389 | 31580648 | 0 | 631 | 359408 | 3 |
|  |  |  | mono | 19927389 | 39934647 | 1 | 499 | 272624 | 3 |
| **best (THE number)** | | | | **mono** | **39934647** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | interp | 9951396 | 6184833 | 1 | 1609 | 1327200 | 3 |
|  |  |  | rxgraph | 9951396 | 35414221 | 0 | 281 | 176816 | 3 |
|  |  |  | mono | 9951396 | 43838749 | 0 | 227 | 155520 | 3 |
|  |  |  | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **43838749** | | | | |
| 100000 | layers=6 width=1250 fanout=16 | 100000 | interp | 10403068 | 3043613 | 11 | 3418 | 2567056 | 3 |
|  |  |  | rxgraph | 10403068 | 14349059 | 5 | 725 | 1088048 | 3 |
|  |  |  | mono | 10403068 | 21763741 | 8 | 478 | 247888 | 3 |
| **best (THE number)** | | | | **mono** | **21763741** | | | | |
| 1000000 | layers=4 width=15000 fanout=8 | 360000 | interp | 9815343 | 10966864 | 50 | 895 | 1512336 | 3 |
|  |  |  | rxgraph | 9815343 | 36899786 | 21 | 266 | 461840 | 3 |
|  |  |  | mono | 9815343 | 50078281 | 36 | 196 | 229664 | 3 |
| **best (THE number)** | | | | **mono** | **50078281** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|
| interp | 498016 | 0.1 |
| rxgraph | 479280 | 0.1 |
| mono | 473424 | 0.1 |


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
