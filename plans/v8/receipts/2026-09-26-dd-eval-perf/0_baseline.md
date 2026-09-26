# DD evaluator baseline

`CARGO_TARGET_DIR=/Users/chrishafley/.agent/lanes/feature-eval-on-dd/verify-target`, `cargo build --offline -j 4 --bin dl8`, then three serial direct-binary runs per fixture and engine with `/usr/bin/time -l`. Debug profile, 2026-09-26. DD and Rust JSON matched in every run.

| fixture (`dl8 compile --trace`) | DD wall, s | Rust wall, s | DD calls | DD build, ms | DD step, ms | DD total, ms |
|---|---:|---:|---:|---:|---:|---:|
| `fixtures/comptime_effect/0_fs_json.dl7` | 5.12, 4.99, 5.04 | 0.14, 0.14, 0.14 | 6 | 468 | 4107 | 4940 |
| `fixtures/declare/0_primitive_defaults.dl7` (`_8` case) | 3.03, 3.07, 3.06 | 0.12, 0.12, 0.12 | 4 | 293 | 2356 | 2892 |

DD phase figures use the first sample. `DD total` includes stratification, setup, and dataflow teardown. Row extraction registered below 1 ms after teardown in each call.

| fixture | stratum 0 | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---:|---:|---:|---:|---:|---:|---:|
| `0_fs_json` step ms | 549 | 332 | 639 | 496 | 487 | 914 | 690 |
| `0_primitive_defaults` step ms | 329 | 190 | 369 | 281 | 285 | 506 | 396 |
