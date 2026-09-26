# SQLite evaluator case status

Final per-file runs used `DL8_ENGINE=sqlite`, `RUST_LOG=dl8=debug`, the supplied `SQLITE_IVM_LIB`, and `cargo nextest run --offline --locked --no-fail-fast --test <file>`. Nextest retained its 60-second per-test bound.

| file | pass | fail | timeout | wall |
|---|---:|---:|---:|---:|
| `_27_eval_sqlite` | 1 (32 oracle cases) | 0 | 0 | 9.949s |
| `_18_sqlite_emit` | 17 | 0 | 0 | 33.935s |
| `_6_comptime_oracle` | 3 | 0 | 0 | 2.839s |
| `_8_compile_oracle` | 3 | 0 | 0 | 35.152s |
| `_15_fold` | 0 | 2 | 0 | 7.356s |
| `_23_comptime_effect` | 4 | 0 | 0 | 6.080s |

Combined exact recipe body: 28 pass, 2 fail, 0 timeout; its slow compile test took 53.275s. No CI configuration or test coverage changed.

Blocked at `src/_5_reify/_7_sqlite.rs:1098-1120`: declared folds lower only when their step is `int.add` with an integer seed. `fixtures/fold/1_program_step.dl7` supplies `Input(7)`, `Input(35)`, `Input(100)` and a `Doubling` step; Rust produces `Total(284)`, while SQLite returns no fixture-owned rows. `fixtures/fold/2_order_matters.dl7` expects `Total(12)` and gets no fixture-owned rows. `fixtures/fold/3_step_no_row.dl7` expects `fold_step_no_row` after its second value, while SQLite emits `not_built_yet(fold)`. Rust's program-step behavior is at `src/_6_eval/_5_evaluate.rs:611-710`; the integration assertions are in `tests/_15_fold.rs:114-161`.

Approval question: should SQLite execute program-defined fold steps in term order and emit the same missing-step diagnostic as Rust? This changes SQLite evaluation behavior beyond the approved cost fixes.
