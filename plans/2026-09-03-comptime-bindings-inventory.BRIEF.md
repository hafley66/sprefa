# brief: inventory of every host binding dl7 comptime and the dbsp emitter would share

Lane: `docs/comptime-bindings-inventory`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Deliverable: ONE file, `docs/comptime-bindings-inventory.md`. No code, no other file.

## Why

Chris 2026-09-03: "sprefa extract and soopy bindings in the swi prolog compiler comptime for dl7 ... then we are able to use same api when outputting a dbsp program". A planning lane needs the roster before it can design the binding surface. This lane retrieves; it decides nothing.

## What to list, each as a table with file:line for every row

1. Every executor engine-rs links: `v6/sprefa-engine-rs/src/executors/mod.rs` and each file it names. Columns: executor struct, the `sh` host name it answers, input columns, output columns, the crate it calls (soopy, sprefa-extract, scip, cargo_metadata, fixture), file:line of the trait impl.
2. Every `process_create` and every child-process spawn in `v7/src/**` and `v6/prolog/**`: file:line, the binary, the arguments, what reads the output.
3. The soopy CLI surface: `~/projects/hafley-rs/crates/soopy/README.md` and `src/main.rs` (or the bin file `Cargo.toml` names): subcommand, flags, output format (jsonl or not), the lib fn behind it (file:line in soopy `src/`).
4. The extract CLI surface relevant to types and facts: `v6/sprefa-extract/src/bin/extract.rs` flags (`--witness`, `--resolve`, `--family`, `--ts-checker`, `--rust-checker`, `--project-root`, `--ingest`, `--schema`), the record kinds on the wire (`src/tsi/types.rs`, `src/wire.rs` `FlatFact` variants), and the registry row count from `extract --schema | wc -l` (build with `cargo build --features cli`; the path dep `hafley-observe` needs a `hafley-rs` symlink beside your worktree's parent dir pointing at `/Users/chrishafley/projects/hafley-rs/.worktrees/origin-main` if cargo cannot find `../../../hafley-rs`).
5. The dl7 comptime phases: `v7/src/2_comptime/2_compiler.pl` `run_compile_phase` call sites in order, what each loads (`0c_extract_loader.pl`, `0d_source_fact_loader.pl`, `0b_filesystem_grapher.pl`), and whether anything in `v7/src/1_libtime/0_evaluator.pl` calls out of process (grep `process_create`, `foreign`, `shell`, `ffi`).
6. The dd/dbsp emit surface: `v6/prolog/compile/6_isolated_compiler_dd.pl` `dd_plan` term shape (the top-level functor and its arguments, file:line), `docs/ext-dbsp-incremental.md` sections 1-3 headings, and `v6/dd-runner` (if present) input handles.
7. swipl foreign options available on this machine: `swipl --version`, whether `library(ffi)` loads (`swipl -g "use_module(library(ffi))" -t halt`, paste the result), whether `library(process)` loads.

## Receipts

- Every row carries a file:line you opened. No row from memory.
- `wc -l docs/comptime-bindings-inventory.md` in the PR body.
- The doc opens with a TOC and uses tables only; one mermaid flowchart of the process boundary (which process runs which piece today) is allowed, under 12 shapes.

## Ownership

Owned: `docs/comptime-bindings-inventory.md`. Forbidden: everything else.

## Style laws

No em dashes. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Language vocabulary: rxjs, prolog, SQL words only; "support" is banned. No recommendations, no design judgments: retrieval only.

## Done

Push, PR against `main`, then:
`boop beep --no-wait --as docs-comptime-bindings-inventory sprefa-coordinator "inventory PR #<n>: <rows> rows, <n> executors, <n> spawn sites, ffi loads: <yes|no>"`.
