# dl8

DL7, compiled in Rust. The filesystem is the pipe: every `src/_<n>_name/`
folder is one operator, in order. Tests live in `tests/` only; oracle
fixtures in `oracle/`.

| folder | status |
|---|---|
| `_6_eval` | stratified semi-naive evaluator, parity with v7 `evaluate/4` |

```bash
cargo test                      # oracle parity through the real binary
cargo run -- eval oracle/eval/0_transitive.json --trace
```

Refreeze an oracle fixture (needs swipl and the v7 tree):

```bash
cd oracle/eval && swipl dump_eval.pl -- 0_transitive.pl 0_transitive.json
cd ../../.. && swipl v8/oracle/eval/dump_compile.pl -- v7/test/fixtures/2_partial.dl7 v8/oracle/eval/c2_partial
```

Design: `plans/v8/2026-09-12-v8-eval-api.md`. Buy-vs-build:
`plans/v8/2026-09-12-v8-buy-vs-build.md`.
