# DD evaluator status

Debug profile, serial direct-binary `compile --trace` runs. Medians use three samples per engine. Baseline and change receipts are `0_baseline.md` through `5_fold.md`.

| fixture | baseline DD s | final DD s | final Rust s | DD/Rust |
|---|---:|---:|---:|---:|
| `0_fs_json` | 5.04 | 0.84 | 0.13 | 6.5× |
| `_8` `0_primitive_defaults` | 3.06 | 0.56 | 0.12 | 4.7× |
| declared fold `1_program_step` | 0.39 | 0.36 | 0.12 | 3.0× |

| first-sample DD phase | calls | build ms | stratum step ms | evaluator total ms | wall ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 6 | 121 | 474 | 706 | 840 |
| `_8` `5_curry` | 34 | 1178 | 6825 | 9135 | 9550 |

`_23_comptime_effect` passes 4/4; `_8_compile_oracle` passes 3/3 with its parity test at 19.731 s under the existing 60 s limit. One per-case `_8` run took 18.58 s under DD and 2.27 s under Rust; [6_oracle_cases.tsv](6_oracle_cases.tsv) records all 16 committed cases, 0 within 2×. JSON matched Rust for every timed representative run; the oracle test checks all committed cases. Row extraction registered below 1 ms per evaluator call. The remaining measured time is concentrated in stratum stepping and rebuilding the dataflow on each call. The target ratio remains unmet. No CI coverage was added, changed, or removed.
