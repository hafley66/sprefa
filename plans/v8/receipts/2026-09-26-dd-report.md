# Differential dataflow evaluator report
Floor: 32/32 frozen `oracle/eval` fixtures.
| test file | DD result |
|---|---:|
| `_28_eval_dd` | 2/2 |
| `_0_eval_oracle` | 1/1 |
| `_27_eval_sqlite` | 1/1 |
| `_11_aggregates` | 1/1 |
| `_24_demand_count` | 1/1 |
| `_14_host_effect` | 3/3 |
| `_15_fold` | 2/2 |
| `_18_sqlite_emit` | 17/17 |
| `_6_comptime_oracle` | 3/3 |
| `_8_compile_oracle` | 3/3 |
| `_23_comptime_effect` | 2/4 at default concurrency; 4/4 serialized |
`_23` has no differing row: `0_fs_json` and `3_two_rounds` exceed the 10 second limit at default concurrency. The measured `0_fs_json` compile is 13.2 seconds with DD and 0.19 seconds with Rust in debug builds.
Rule operators: positive goals use keyed `join`; recursive strata use `iterate` and `distinct`; negative goals use `antijoin`; aggregates use `reduce`; kernel goals use `flat_map`; served goals add intern request, effect, and snapshot rows before their join.
Rust evaluator behavior not mirrored: none in the covered fixtures and test files.
