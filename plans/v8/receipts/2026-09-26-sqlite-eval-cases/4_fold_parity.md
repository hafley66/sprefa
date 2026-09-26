# Declared fold parity and final case run

At `src/_6_eval/_7_sqlite_eval.rs:289`, a declared fold becomes a SQL proof relation and an empty seeded result relation. `refresh_folds` at line 207 groups proof rows, passes each group and the SQLite closure to `fold_generic_rows` at line 242, and writes results for dependent rules. The shared helper owns term order and step lookup. A failed step suppresses closure rows and emits its diagnostic; seed updates clear prior fold rows and diagnostics. The SQLite planner and Rust engine were not changed.

Before (rebased `d3954e11e`): `_15_fold` 0/2, 7.356s; combined recipe body 28/30, 2 failures, 0 timeouts. After: `_15_fold` 2/2, 7.092s; combined recipe body 30/30, 0 failures, 0 timeouts, 110.565s wall. Its slowest test, `_8_compile_oracle::every_committed_case_matches_v7`, took 42.935s in the combined run, below the 60s bound.

Per-file warm runs: `DL8_ENGINE=sqlite RUST_LOG=dl8=debug cargo nextest run --offline --locked --no-fail-fast --test <file>` with the supplied `SQLITE_IVM_LIB`, `CARGO_TARGET_DIR=target-sq`, and build jobs 4. Wall is nextest's test-run summary, excluding compile time.

| file | pass | fail | timeout | wall |
|---|---:|---:|---:|---:|
| `_27_eval_sqlite` | 1 (32 oracle cases) | 0 | 0 | 12.443s |
| `_18_sqlite_emit` | 17 | 0 | 0 | 36.196s |
| `_6_comptime_oracle` | 3 | 0 | 0 | 3.498s |
| `_8_compile_oracle` | 3 | 0 | 0 | 40.814s |
| `_15_fold` | 2 | 0 | 0 | 7.092s |
| `_23_comptime_effect` | 4 | 0 | 0 | 5.285s |

`DL8_ENGINE=dd cargo nextest run --offline --locked --no-fail-fast --test _28_eval_dd --test _0_eval_oracle`: 3/3 (DD oracle, served goal, Rust oracle). CI coverage unchanged.
