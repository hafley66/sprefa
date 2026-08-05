# exec_shootout STANDINGS

Run command: `./harness/target/release/harness --engines interp/target/release/interp,rxgraph/target/release/rxgraph,mono/target/release/mono --scales 10000,100000,1000000 --measure-builds`

THE number is derived rows/sec in the fixpoint phase, best of 3.

## chain

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | segment_len=2582 | 7743 | interp | 9996213 | 3054144 | 1 | 3273 | 1485824 | 3 |
|  |  |  | rxgraph | 9996213 | 32455237 | 0 | 308 | 300432 | 3 |
|  |  |  | mono | 9996213 | 1051569 | 0 | 9506 | 300288 | 3 |
|  |  |  | ref | 9996213 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **rxgraph** | **32455237** | | | | |
| 100000 | segment_len=200 | 99898 | interp | 9989800 | 2334065 | 21 | 4280 | 1699328 | 3 |
|  |  |  | rxgraph | 9989800 | 26219948 | 6 | 381 | 329984 | 3 |
|  |  |  | mono | 9989800 | 10593637 | 12 | 943 | 316880 | 3 |
| **best (THE number)** | | | | **rxgraph** | **26219948** | | | | |
| 1000000 | segment_len=20 | 999989 | interp | 9999890 | 1761164 | 354 | 5678 | 1899872 | 3 |
|  |  |  | rxgraph | 9999890 | 17330832 | 67 | 577 | 533184 | 3 |
|  |  |  | mono | 9999890 | 25316177 | 231 | 395 | 481936 | 3 |
| **best (THE number)** | | | | **mono** | **25316177** | | | | |

## grid

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | rows=79 cols=79 | 12324 | interp | 9979359 | 1938492 | 1 | 5148 | 1517168 | 3 |
|  |  |  | rxgraph | 9979359 | 21142710 | 0 | 472 | 373056 | 3 |
|  |  |  | mono | 9979359 | 548106 | 1 | 18207 | 320896 | 3 |
|  |  |  | ref | 9979359 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **rxgraph** | **21142710** | | | | |
| 100000 | rows=79 cols=79 | 12324 | interp | 9979359 | 1936987 | 1 | 5152 | 1276768 | 3 |
|  |  |  | rxgraph | 9979359 | 20747108 | 0 | 481 | 374144 | 3 |
|  |  |  | mono | 9979359 | 476319 | 1 | 20951 | 322800 | 3 |
| **best (THE number)** | | | | **rxgraph** | **20747108** | | | | |
| 1000000 | rows=79 cols=79 | 12324 | interp | 9979359 | 1950236 | 1 | 5117 | 1428656 | 3 |
|  |  |  | rxgraph | 9979359 | 21741523 | 0 | 459 | 376912 | 3 |
|  |  |  | mono | 9979359 | 479800 | 1 | 20799 | 321200 | 3 |
| **best (THE number)** | | | | **rxgraph** | **21741523** | | | | |

## layered

| scale | tuned params | edges | engine | derived | fp rows/sec | load ms | fp ms | peak rss kb | runs |
|---|---|---|---|---|---|---|---|---|---|
| 10000 | layers=193 width=26 fanout=2 | 9984 | interp | 9951396 | 2175644 | 1 | 4574 | 1598896 | 3 |
|  |  |  | rxgraph | 9951396 | 20308971 | 0 | 490 | 345088 | 3 |
|  |  |  | mono | 9951396 | 374676 | 1 | 26560 | 314384 | 3 |
|  |  |  | ref | 9951396 | (reference) | - | - | - | 1 |
| **best (THE number)** | | | | **rxgraph** | **20308971** | | | | |
| 100000 | layers=6 width=1250 fanout=16 | 100000 | interp | 10403068 | 878044 | 12 | 11848 | 2256048 | 3 |
|  |  |  | rxgraph | 10403068 | 8597577 | 6 | 1210 | 1209600 | 3 |
|  |  |  | mono | 10403068 | 69185 | 10 | 150365 | 333312 | 3 |
| **best (THE number)** | | | | **rxgraph** | **8597577** | | | | |
| 1000000 | layers=4 width=15000 fanout=8 | 360000 | interp | 9815343 | 2955538 | 70 | 3321 | 1498400 | 3 |
|  |  |  | rxgraph | 9815343 | 24912038 | 22 | 394 | 531472 | 3 |
|  |  |  | mono | 9815343 | 3165219 | 48 | 3101 | 350752 | 3 |
| **best (THE number)** | | | | **rxgraph** | **24912038** | | | | |

## Engine builds

| engine | release binary size (bytes) | cold build seconds |
|---|---|---|
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |
| interp | 494672 | 0.1 |
| rxgraph | 478320 | 0.1 |
| mono | 472144 | 0.1 |


Correctness: every engine agrees on (derived, checksum); the internal reference anchors truth at the 10k cases. No standings are written from a run with a mismatch.
