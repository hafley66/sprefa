# Release timings and DD stratum counts

Same two fixtures as `0_baseline.md`. `cargo build --release --offline -j 4 --bin dl8` used the existing target directory. Three serial direct-binary `compile --trace` samples per engine were timed with `time.perf_counter`; [release_samples.tsv](release_samples.tsv) has each sample. JSON output matched Rust for each paired sample.

| fixture | release DD median s | release Rust median s | DD/Rust | debug DD/Rust from `7_status.md` |
|---|---:|---:|---:|---:|
| `0_fs_json` | 0.659 | 0.119 | 5.6× | 6.5× |
| `_8` `0_primitive_defaults` | 0.441 | 0.105 | 4.2× | 4.7× |

`DL8_DD_PROFILE=1` on the debug binary records Timely `Operates` events and inspects each stratum's settled collection. Operator counts exclude the measurement-only round inspector and row inspector. Rounds count inner timestamps at which the iteration's `current` collection received updates, including the initial timestamp. Net new rows count positive-weight rows in each settled collection beyond the preceding stratum, including seed rows. Tables sum one profiled compile's evaluator calls. The profile adds inspection and logging, so these runs are used for counts rather than wall-time comparisons.

| `0_fs_json` level | calls | rounds with updates | operators built | net new rows |
|---|---:|---:|---:|---:|
| 0 | 6 | 12 | 9,579 | 2,851 |
| 1 | 3 | 6 | 2,433 | 1,910 |
| 2 | 3 | 6 | 8,574 | 396 |
| 3 | 3 | 6 | 450 | 0 |
| 4 | 3 | 6 | 360 | 0 |
| 5 | 3 | 9 | 8,517 | 2,325 |
| 6 | 3 | 6 | 237 | 0 |

| `_8` `0_primitive_defaults` level | calls | rounds with updates | operators built | net new rows |
|---|---:|---:|---:|---:|
| 0 | 4 | 9 | 6,386 | 1,599 |
| 1 | 2 | 4 | 1,622 | 1,051 |
| 2 | 2 | 4 | 5,716 | 252 |
| 3 | 2 | 4 | 300 | 0 |
| 4 | 2 | 4 | 240 | 0 |
| 5 | 2 | 4 | 5,848 | 1,312 |
| 6 | 2 | 4 | 158 | 0 |

Level 0 includes macrotime and comptime calls; higher levels occur in comptime only. The largest single comptime call in `0_fs_json` builds 2,858 operators at level 2 for 132 new rows and 2,839 at level 5 for 776 new rows. Most levels have two rounds with updates. Release optimization narrows the gap but leaves both fixtures above 2×. The next change should target operator construction and scheduling within a stratum.
