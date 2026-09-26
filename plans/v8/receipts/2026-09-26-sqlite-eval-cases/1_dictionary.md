# Dictionary flush on generated SQL demand

`test-fixtures-5_curry` is the slowest committed compile case. A 20-second `dl8::sql` trace before the change recorded 10 `declare_view` calls totaling 12,317 ms, 11 `flush_terms`/`flush_term_args` pairs totaling 3,240 ms, and 2,684 `nonlinear_round` calls totaling 2,672 ms. One round sequence changed 22, 42, 24, then 0 rows. Sites: `_7_sqlite_eval.rs:130-156`, `:360-379`, `:1048-1110`.

The captured `c2_partial_3` replay gives statement detail: view install 776.55 ms; two nonlinear rounds 235.89 ms total. In round 1, statement 8 took 104.15 ms for 1 inserted row and 2,944 SQLite VM steps; `EXPLAIN QUERY PLAN` reports `SCAN program VIRTUAL TABLE INDEX 0` and indexed searches on `main.n444_a3` and `main.n2212_m2`. Statement 1 took 54.85 ms, 1 row, 2,942 VM steps, the same virtual-table scan and indexed source searches. Both reported 0 full-scan steps and 0 automatic indexes. Material tables held 8 rows after round 1 and 8 after round 2; changed rows were 8 then 0. Capture: `oracle/eval/replay/c2_partial_3/{2_view.sql,3_rounds.json}` and `examples/0_c2_sqlite_replay.rs:82-111` (temporary profiling used `StatementStatus` and was reverted).

The evaluator now creates and flushes `main.sym`, `main.term`, and `main.term_arg` only when the generated view or round SQL names them. It always creates `main.unit`. Sites: `_7_sqlite_eval.rs:280-290`, `:367-373`, `:535-560`. These tables are still available for generated term-order and payload joins.

| check | before | after |
|---|---:|---:|
| `_27_eval_sqlite` (SQLite, `RUST_LOG=off`) | 32/32, 19.581s | 32/32, 7.442s |
| `_8_compile_oracle` alone (SQLite, `RUST_LOG=off`) | 1 test timed out at 60.010s | 3/3, slow test 52.558s |
| `_8_compile_oracle` alone (SQLite, `RUST_LOG=dl8=debug`) | not measured | 3/3, slow test 57.101s |

The exact combined recipe body still times out `_8_compile_oracle` at 60.007s when it runs beside another test; `_15_fold` fails two parity tests. `_23_comptime_effect` passes 4/4 with the recipe's `RUST_LOG=dl8=debug`.
