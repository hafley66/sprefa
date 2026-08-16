---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
epic: extract-port-closeout
labels:
- pkg:extract
- size:med
---

# SCIP indexer roster: port python, kotlin/java and cpp

## Description

## Description

v5's SCIP indexer roster has six languages; v6's has three. python,
kotlin/java and cpp are undetectable and unindexable from v6, so a root with
only those markers gets no real-SCIP tier at all.

## Receipts

| lang | v5 row | v6 |
|---|---|---|
| rust | `src/scip_setup.rs:52-58` | present, `v6/sprefa-extract/src/scip_ensure.rs:66-72` |
| typescript | `src/scip_setup.rs:59-65` | present, `scip_ensure.rs:73-79` |
| go | `src/scip_setup.rs:73-79` | present, `scip_ensure.rs:80-86` |
| python | `src/scip_setup.rs:66-72` (`scip-python`, markers pyproject.toml/setup.py/requirements.txt) | MISSING |
| kotlin/java | `src/scip_setup.rs:80-86` (`scip-java`, markers build.gradle.kts/build.gradle/pom.xml) | MISSING |
| cpp | `src/scip_setup.rs:87-99` (`scip-clang`, markers compile_commands.json/CMakeLists.txt) | MISSING |

The gap is already named in the module header at
`v6/sprefa-extract/src/scip_ensure.rs:35-40`: "each is one `build` body plus its
staging decision, not a new wire" — the decode is indexer-agnostic
(`src/scip_decode.rs`).

## Fix shape

Per language, one `ScipSource` impl in `src/scip.rs` beside `ScipRust` (:140),
`ScipTypescript` (:97), `ScipGo` (:178), plus one `Indexer` row in
`scip_ensure.rs:65`. Every `build` body goes through
`scip_ensure::run_capped` (process-group kill on the deadline,
`src/scip.rs:31-35`), never `Command::output()`.

Each impl must state its STAGING decision in its header comment, the way the
three existing ones do: does the indexer write into the source dir?
- `scip-python` writes an `index.scip` at `--output` and nothing else; probe it.
- `scip-java` drives gradle/maven, which write `build/` and `target/` under the
  root: stage, like `ScipRust`.
- `scip-clang` reads `compile_commands.json` and writes only `-o`.

v5 argv, verbatim (`src/scip_setup.rs` rows above):
- `scip-python index . --output {out}`
- `scip-java index --output {out}`
- `scip-clang --compdb-path compile_commands.json -o {out}`

A missing binary must stay a named skip (`scip_skip` row, exit 0), never a
failure — `scip_ensure.rs:9-12`.

## Gate

```bash
cd v6/sprefa-extract
cargo build --all-targets --features cli
cargo test --features cli
cargo test --features cli --test 8_scip_families_cli
```
Toolchain-absent path must be covered by a test that asserts a `scip_skip` row
and rc=0 for a marker-bearing root with no binary on PATH.
