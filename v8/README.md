# dl8

DL7, compiled in Rust. The filesystem is the pipe: every `src/_<n>_name/`
folder is one operator, in order. Tests live in `tests/` only; oracle
fixtures in `oracle/`.

| folder | status |
|---|---|
| `_0_read` | `read_dl7/5` port over a character stream, 89 oracle fixtures |
| `_1_macrotime` | reify, `<+` expansion waves over `_6_eval`, materialize, 29 oracle cases |
| `_2_lower` | `0_lowerer.pl` and both graph stores, 19 committed of 62 checked lowerings |
| `_3_check` | `1_checker.pl` port, 19 committed of 75 checked calls, 25 diagnostic functors classified |
| `_4_comptime` | the two nested compiler fixpoints, `2_compiler.pl:700-1591`; `_7_sources.rs` is the live refreeze, `Replay` the oracle one; `_0_load` ports the filesystem, TSI and source-fact loaders, 51 committed of 57 cases |
| `_5_reify` | `0_logical_program_reifier.pl`, `0a_logical_program_grapher.pl`, `1_artifact_emitter.pl`; 57 committed of 88 checked calls |
| `_6_eval` | stratified semi-naive evaluator, parity with v7 `evaluate/4` |
| `_8_driver` | the call in order, `2_compiler.pl:75-700`; `lib.rs::compile` is the chain, 16 committed of 46 whole-pipeline cases |

`prelude/` and `macrotime/` are byte-identical copies of `v7/prelude` and
`v7/macrotime` at `f5018ad23`, compiled into the binary with `include_str!`, so
nothing under `v7/` is read at runtime.

```bash
cargo test                      # oracle parity through the real binary
cargo run -- compile ../v7/test/fixtures/2_partial.dl7
cargo run -- compile oracle/compile/sources/test/fixtures/modules/0_accounts.dl7 \
  oracle/compile/sources/test/fixtures/modules/1_consumer.dl7 \
  --project oracle/compile/sources/test/fixtures/modules
cargo run -- eval oracle/eval/0_transitive.json --trace
```

Refreeze an oracle fixture (needs swipl and the v7 tree):

```bash
cd oracle/eval && swipl dump_eval.pl -- 0_transitive.pl 0_transitive.json
cd ../../.. && swipl v8/oracle/eval/dump_compile.pl -- v7/test/fixtures/2_partial.dl7 v8/oracle/eval/c2_partial
cd v8/oracle/read && V7_DIR=/Users/chrishafley/projects/sprefa/v7 swipl dump_read.pl --
V7_DIR=$PWD/v7 bash v8/oracle/macrotime/refreeze.sh
V7_DIR=v7 bash v8/oracle/lower/freeze.sh
bash v8/oracle/check/freeze.sh            # pins REV=f5018ad23
V7_DIR=v7 bash v8/oracle/load/freeze.sh
bash v8/oracle/reify/freeze.sh
bash v8/oracle/compile/freeze.sh          # pins REV=f5018ad23, all 46 cases
bash v8/oracle/compile/verify.sh          # dl8 over every case, committed or not
```

Design: `plans/v8/2026-09-12-v8-eval-api.md`. Buy-vs-build:
`plans/v8/2026-09-12-v8-buy-vs-build.md`.
