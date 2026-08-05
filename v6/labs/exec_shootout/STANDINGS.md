# exec_shootout STANDINGS

Run command: `./harness/target/release/harness --engines interp/target/release/interp,rxgraph/target/release/rxgraph,mono/target/release/mono --scales 10000,100000,1000000 --measure-builds --work /private/tmp/claude-501/-Users-chrishafley-projects-sprefa/92b4dd87-37e6-487b-9aee-eb00071bff32/scratchpad/ladder`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | interp | 9996213 | 3508674 | 1 | 2849 | 1585376 | 3 |
|  |  |  | rxgraph | 9996213 | 34830010 | 0 | 287 | 301408 | 3 |
|  |  |  | mono | 9996213 | 62088280 | 0 | 161 | 117104 | 3 |
|  |  |  | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **62088280** | | | | |
| 100000 | segment_len=200 | 99898 | interp | 9989800 | 2598803 | 17 | 3844 | 1799184 | 3 |
|  |  |  | rxgraph | 9989800 | 30272121 | 5 | 330 | 330528 | 3 |
|  |  |  | mono | 9989800 | 40444534 | 13 | 247 | 187232 | 3 |
| **best (THE number)** | | | | **mono** | **40444534** | | | | |
| 1000000 | segment_len=20 | 999989 | interp | 9999890 | 2022222 | 341 | 4945 | 2360736 | 3 |
|  |  |  | rxgraph | 9999890 | 18416004 | 64 | 543 | 569680 | 3 |
|  |  |  | mono | 9999890 | 18587156 | 223 | 538 | 437712 | 3 |
| **best (THE number)** | | | | **mono** | **18587156** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=45 cols=45 | 3960 | interp | 1069200 | 2813684 | 0 | 380 | 219888 | 3 |
|  |  |  | rxgraph | 1069200 | 34490323 | 0 | 31 | 39376 | 3 |
|  |  |  | mono | 1069200 | 62894118 | 0 | 17 | 14496 | 3 |
|  |  |  | ref | 1069200 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **62894118** | | | | |
| 100000 | rows=65 cols=65 | 8320 | interp | 4596800 | 2401672 | 1 | 1914 | 950512 | 3 |
|  |  |  | rxgraph | 4596800 | 19233473 | 1 | 239 | 192160 | 3 |
|  |  |  | mono | 4596800 | 25396685 | 2 | 181 | 84720 | 3 |
| **best (THE number)** | | | | **mono** | **25396685** | | | | |
| 1000000 | rows=94 cols=94 | 17484 | interp | 19927389 | 2006180 | 4 | 9933 | 2833472 | 3 |
|  |  |  | rxgraph | 19927389 | 21109522 | 1 | 944 | 700128 | 3 |
|  |  |  | mono | 19927389 | 36035061 | 1 | 553 | 312096 | 3 |
| **best (THE number)** | | | | **mono** | **36035061** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | interp | 9951396 | 2413630 | 1 | 4123 | 1603808 | 3 |
|  |  |  | rxgraph | 9951396 | 24450604 | 0 | 407 | 342784 | 3 |
|  |  |  | mono | 9951396 | 43266939 | 0 | 230 | 141440 | 3 |
|  |  |  | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **mono** | **43266939** | | | | |
| 100000 | layers=6 width=1250 fanout=16 | 100000 | interp | 10403068 | 1036369 | 10 | 10038 | 3379856 | 3 |
|  |  |  | rxgraph | 10403068 | 9936073 | 6 | 1047 | 1160000 | 3 |
|  |  |  | mono | 10403068 | 21361536 | 8 | 487 | 224144 | 3 |
| **best (THE number)** | | | | **mono** | **21361536** | | | | |
| 1000000 | layers=4 width=15000 fanout=8 | 360000 | interp | 9815343 | 2215653 | 84 | 4430 | 1779056 | 3 |
|  |  |  | rxgraph | 9815343 | 30294269 | 21 | 324 | 523968 | 3 |
|  |  |  | mono | 9815343 | 48832552 | 35 | 201 | 221264 | 3 |
| **best (THE number)** | | | | **mono** | **48832552** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|
| interp | 494672 | 0.0 |
| rxgraph | 478320 | 0.0 |
| mono | 473424 | 0.0 |


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
