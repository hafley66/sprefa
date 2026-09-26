# Exact seed-state view reuse

The `5_curry` trace showed one rule fingerprint repeated with seed counts alternating between 1,437 and 1,454. The previous one-entry cache rebuilt after each retraction because its only entry held the other seed set. `_7_sqlite_eval.rs:1160-1289` now retains two bounded engine entries. It selects an exact rule and seed match before the append-only delta path; a retraction with no exact match still builds a fresh view.

| check | before this change | after |
|---|---:|---:|
| isolated `5_curry` compile, `RUST_LOG=off` | 29s | 10s |
| exact recipe body, `_8_compile_oracle` slow test | timeout at 60.007s | pass at 53.275s |
| exact recipe body, all tests | 27 pass, 2 fail, 1 timeout | 28 pass, 2 fail, 0 timeout |

The `5_curry` trace after the change records 25 exact-state reuses and eight view declarations. `_0_eval_oracle` and `_27_eval_sqlite` pass. `DL8_EVAL_CHECK=1` on `5_curry` completed with zero fresh-only and cached-only rows, and its output SHA-256 matched the ordinary run: `b2c71da4663134d6abe73b9a9830e13adc4a0dd0e78bec4b67fba187fa9b643c`.

Two more isolated `_8_compile_oracle` runs with recipe logging passed in 35.481s and 34.543s.
