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

## mercury-semi-naive additive results

| metadata | value |
|---|---|
| machine | Apple M2 Pro, arm64, macOS 14.6.1 (23G93) |
| date | 2026-08-14 |
| measured runs | 3 per case, best fixpoint time selected |
| compiler | Mercury 22.01.8, `mmc -O5 --make` |

Run command: `./target/release/harness --engines ../mercury-semi-naive/mercury-semi-naive --scales 10000,100000,1000000 --work /tmp/mercury-final.MuvZQx --standings /tmp/mercury-final.MuvZQx/standings.md`

The `--engines` path list is the harness registration point. The harness has
no static engine registry.

| family | scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|---|
| chain | 10000 | segment_len=2582 | 7743 | mercury-semi-naive | 9996213 | 24203906 | 1 | 413 | 19712 | 3 |
| chain | 100000 | segment_len=200 | 99898 | mercury-semi-naive | 9989800 | 19664961 | 14 | 508 | 54944 | 3 |
| chain | 1000000 | segment_len=20 | 999989 | mercury-semi-naive | 9999890 | 13908053 | 234 | 719 | 252080 | 3 |
| grid | 10000 | rows=45 cols=45 | 3960 | mercury-semi-naive | 1069200 | 16200000 | 1 | 66 | 15248 | 3 |
| grid | 100000 | rows=65 cols=65 | 8320 | mercury-semi-naive | 4596800 | 13972036 | 1 | 329 | 24416 | 3 |
| grid | 1000000 | rows=94 cols=94 | 17484 | mercury-semi-naive | 19927389 | 12700694 | 5 | 1569 | 48432 | 3 |
| layered | 10000 | layers=193 width=26 fanout=2 | 9984 | mercury-semi-naive | 9951396 | 15333430 | 2 | 649 | 24768 | 3 |
| layered | 100000 | layers=6 width=1250 fanout=16 | 100000 | mercury-semi-naive | 10403068 | 3248928 | 18 | 3202 | 406192 | 3 |
| layered | 1000000 | layers=4 width=15000 fanout=8 | 360000 | mercury-semi-naive | 9815343 | 8609950 | 254 | 1140 | 925872 | 3 |

| engine build metadata | bytes | cold build seconds | machine | date | runs |
|---|---|---|---|---|---|
| mercury-semi-naive | 76448 | 0.49 | Apple M2 Pro | 2026-08-14 | 1 |

Checksum validation before timing:

| case | derived | checksum | compared engines | machine | date | runs |
|---|---|---|---|---|---|---|
| chain 10000 | 9996213 | `df09b2f409f8b9a8` | mono, mercury-semi-naive | Apple M2 Pro | 2026-08-14 | 1 each |
| grid 10000 | 1069200 | `9d7239568960d6a8` | mono, mercury-semi-naive | Apple M2 Pro | 2026-08-14 | 1 each |
