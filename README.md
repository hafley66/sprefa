# dl8

DL7, compiled in Rust. Every `src/_<n>_name/` folder is one phase, in order.
Tests live in `tests/it/` (one integration binary); oracle goldens in `oracle/`.

| folder | phase |
|---|---|
| `_0_read` | the reader, characters to reader node terms |
| `_1_macrotime` | reader forms to a syntax graph, `<+` rewrite waves, back to forms |
| `_2_lower` | the lowerer and its graph stores |
| `_3_check` | the checker |
| `_4_comptime` | the compiler fixpoint; `_0_load` loads the filesystem, TSI, OpenAPI and source facts |
| `_5_reify` | the logical program reifier, grapher and artifact emitter |
| `_6_eval` | datalog evaluation through `ivm_engine::Engine`; the one connected engine is `ir:dd` |
| `_7_effect.rs` | one reducer shape for the fixpoints, effects leave through `fx` |
| `_8_driver` | the call in order; `lib.rs::compile` is the chain |
| `_9_runtime` | the closure at rest, the run loop and its executors |

The module headers (`//!`) name the v7 Prolog each phase was ported from.
`prelude/`, `macrotime/` and `std/` are compiled into the binary with
`include_str!`. `crates/tree-sitter-dl7/` is the grammar. `sqlite_ivm` and
`hafley-rs` are gitignored symlinks to sibling checkouts; `just boop-start`
makes them. Earlier engines live under `v5/`, `v6/`, `v7/` and take no work.

```bash
cargo test                                   # tests/it through the real binary
cargo run -- compile tests/fixtures/2_partial.dl7
cargo run -- compile oracle/compile/sources/test/fixtures/modules/0_accounts.dl7 \
  oracle/compile/sources/test/fixtures/modules/1_consumer.dl7 \
  --project oracle/compile/sources/test/fixtures/modules
bash oracle/refreeze.sh                      # regenerate every golden from dl8
```

Environment variables: `docs/0_observability.md`.

## Commands

`dl8 --help` lists them. The ones a program author uses:

| command | what |
|---|---|
| `compile <file.dl7> [--project <root>] [--tsi <jsonl>] [--trace]` | compiler rows, runtime program, diagnostics as JSON |
| `build <file.dl7> [--out <dir>] [--check] [--project <root>] [--tsi <jsonl>]` | compile, then write every `fs.file` row |
| `eval <program.json> [--serve ...] [--db <file>]` | evaluate a checked-goal program |
| `run <compile.json> --serve <names> [--db <file>] [--max-ticks N]` | run as a process: each tick evaluates, hands `effect` rows to executors, inserts answers |
| `query <file.dl7> [--repo <dir>] [--format table\|json]` | compile one query against a repository and print the root file's rows |

## OpenAPI documents

An `oai.document` seed row names a JSON document; the driver reads those rows
after lowering (`src/lib.rs:421`, `src/_8_driver/_5_import.rs:205`) and
`src/_4_comptime/_0_load/_9_openapi.rs` writes each document as TSI rows
(`install_openapi_graph`). There is no `--openapi` flag. Tests:
`tests/it/_6_imports.rs`, `tests/it/_8_contracts.rs`.

## dl8 run

The loop is `src/_9_runtime/_2_reconcile.rs`; executors are
`src/_9_runtime/_3_executors/`. A served name needs every companion relation
its executor writes declared, or `--serve` fails with
`executor_relation_unknown` (`executors_for`, `_3_executors/mod.rs:204`).

| served name | companions it writes | cadence | declared in |
|---|---|---|---|
| `timer` | | Continuing | the program |
| `http.fetch` | | Once | `std/http.dl7` |
| `fetch_json` | `fetch_json_error` | Once | `std/http.dl7` |
| `http.serve` | `http.request`, `http.response`, `http.serve_error` | Continuing | `std/http.dl7` |
| `git.refs` | `git.refs_error` | Continuing | `std/git.dl7` |
| `git.history` | `git.history_error` | Once | `std/git.dl7` |
| `fs.at` | `fs.at_error` | Once | `std/fs.dl7` |
| `fs.json` | `fs.json_error`, `none` | Once | `std/fs.dl7` |
| `fs.watch` | | Continuing | `std/fs.dl7` |
| `doc.layout` | | Once | `std/doc.dl7` |
| `lsp.stdin` | `lsp.buffer`, `lsp.hover_at` | Continuing | `std/lsp.dl7` |
| `extract` | `extract_error` | Once | the program |
| `ryi.resolve` | `ryi.resolve_error` | Once | `std/ryi.dl7` |

Names come from each executor's `RELATION` constant; cadence from its
`cadence()`. Tests: `tests/it/_5_store_host.rs`, `tests/it/_11_watchers.rs`.

`extract` runs `$SPREFA_EXTRACT_BIN` when set, else `ryi` on `PATH`
(`_3_executors/extract.rs:158-170`).

```bash
cargo run -- compile fixtures/reconcile/0_timer.dl7 > target/timer.json
cargo run -- run target/timer.json --serve timer --max-ticks 3
```

## Logs

Every phase emits one `dl8::phase` event with its name and milliseconds.
Tracing goes to stderr; stdout stays the JSON output.

```bash
RUST_LOG=dl8=debug dl8 compile f.dl7
HAFLEY_LOG_FORMAT=json dl8 compile f.dl7
dl8 compile f.dl7 --trace                 # raises the default filter to dl8=debug
```
