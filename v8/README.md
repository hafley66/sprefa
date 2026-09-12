# dl8

DL7, compiled in Rust. The filesystem is the pipe: every `src/_<n>_name/`
folder is one operator, in order. Tests live in `tests/` only; oracle
fixtures in `oracle/`.

| folder | status |
|---|---|
| `_0_read` | `read_dl7/5` port over a character stream, 89 oracle fixtures |
| `_1_macrotime` | reify, `<+` expansion waves over `_6_eval`, materialize, 29 oracle cases |
| `_2_lower` | `0_lowerer.pl` and both graph stores, 19 committed of 62 checked lowerings |
| `_3_check` | not built yet |
| `_4_comptime` | not built yet, `_0_load` holds the filesystem, TSI and source-fact loaders |
| `_5_reify` | not built yet |
| `_6_eval` | stratified semi-naive evaluator, parity with v7 `evaluate/4` |

```bash
cargo test                      # oracle parity through the real binary
cargo run -- eval oracle/eval/0_transitive.json --trace
```

Refreeze an oracle fixture (needs swipl and the v7 tree):

```bash
cd oracle/eval && swipl dump_eval.pl -- 0_transitive.pl 0_transitive.json
cd ../../.. && swipl v8/oracle/eval/dump_compile.pl -- v7/test/fixtures/2_partial.dl7 v8/oracle/eval/c2_partial
cd v8/oracle/read && V7_DIR=/Users/chrishafley/projects/sprefa/v7 swipl dump_read.pl --
V7_DIR=$PWD/v7 bash v8/oracle/macrotime/refreeze.sh
V7_DIR=v7 bash v8/oracle/lower/freeze.sh
```

Design: `plans/v8/2026-09-12-v8-eval-api.md`. Buy-vs-build:
`plans/v8/2026-09-12-v8-buy-vs-build.md`.
