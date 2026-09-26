# SQLite evaluator cases baseline

Environment: `DL8_ENGINE=sqlite`, `SQLITE_IVM_LIB=/Users/chrishafley/projects/sqlite_ivm/target/extension/release/libsqlite_ivm.dylib`, `CARGO_TARGET_DIR=$PWD/target-sq`, `CARGO_BUILD_JOBS=4`, `RUST_TEST_THREADS=4`. Each file ran separately with `cargo nextest run --offline --build-jobs 4 -j 4 --no-fail-fast --test <file>`. Nextest caps each test at 60 seconds. Durations below are nextest suite wall time and exclude compilation. The first build took 5m 50s.

| file | pass | fail | timeout | wall |
|---|---:|---:|---:|---:|
| `_27_eval_sqlite` | 1 (32 oracle cases) | 0 | 0 | 19.585s |
| `_18_sqlite_emit` | 17 | 0 | 0 | 51.059s |
| `_6_comptime_oracle` | 3 | 0 | 0 | 8.856s |
| `_8_compile_oracle` | 2 | 0 | 1 (`every_committed_case_matches_v7`) | 60.022s |
| `_15_fold` | 0 | 2 | 0 | 19.156s |
| `_23_comptime_effect` | 1 | 3 | 0 | 6.996s |

`_15_fold`: `1_program_step` and `2_order_matters` returned no owned rows; `3_step_no_row` returned `not_built_yet(fold)` instead of `fold_step_no_row`. `_23_comptime_effect`: three tests missed expected answer or stable-round rows. Logs: `target-sq/baseline/*.log` (excluded build directory).
