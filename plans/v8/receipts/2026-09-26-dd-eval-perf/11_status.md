# DD evaluator performance status

Serial direct-binary measurements on 2026-09-26 in the existing target directory. Each representative wall time is the median of three `compile --trace` runs per engine. The release build used `cargo build --release --offline -j 4 --bin dl8`. DD and Rust JSON matched in each representative run.

| fixture | baseline debug DD s | current debug DD s | current debug Rust s | current release DD s | current release Rust s | release DD/Rust |
|---|---:|---:|---:|---:|---:|---:|
| `0_fs_json` | 5.04 | 0.648 | 0.13 | 0.526 | 0.119 | 4.4× |
| `_8` `0_primitive_defaults` | 3.06 | 0.448 | 0.12 | 0.339 | 0.105 | 3.2× |

`_23_comptime_effect` passes 4/4 below its existing 10 s compile limit. `_8_compile_oracle` passes 3/3; its parity test took 12.434 s under the existing 60 s limit. [11_oracle_cases.tsv](11_oracle_cases.tsv) records one serial debug sample for each of the 16 committed `_8` cases, paired with Rust JSON and exit-code parity. Six cases are within 2× Rust, compared with zero in `6_oracle_cases.tsv`. The slowest is `5_curry`: 6.532 s DD versus 0.456 s Rust.

| first-sample DD phase | evaluator calls | dataflow build ms | stratum step ms | evaluator total ms | compile wall ms |
|---|---:|---:|---:|---:|---:|
| `0_fs_json` | 6 | 131 | 445 | 702 | 927 |
| `_8` `5_curry` | 34 | 966 | 4,345 | 6,278 | 6,724 |

The phase sample ran after the representative medians and had a higher `0_fs_json` wall time; phase sums are from that single trace. In `5_curry`, level 2 consumed 1,588 ms of step time and level 5 consumed 1,591 ms across 32 comptime calls. Repeated dataflow construction consumed 966 ms. The release build narrows the gap but does not reach the 2× target on either representative fixture. Operator and row counts for both representatives are in `8_release_strata.md` through `10_stored_kernel.md`. No CI coverage was added, changed, or removed.
