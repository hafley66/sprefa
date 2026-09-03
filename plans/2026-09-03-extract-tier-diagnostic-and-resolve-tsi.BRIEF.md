# brief: a declined checker tier is a diagnostic record; --resolve carries the syntax tsi rows

Lane: `fix/extract-tier-diagnostic`. Base: `origin/main` (coordinator states the sha).
FIRST ACTION: `git merge --ff-only <sha>`. Failure = STOP AND REPORT.
Crate: `v6/sprefa-extract`. Paths relative to it unless they start with `v6/` or `docs/`.

Two ARCH rows, one lane, one PR, two commits (one per row, in this order).

## Row 1: `extract_tier_decline_is_a_diagnostic`

`v6/prolog/ARCH.pl:997`. Hand use 2026-09-03: `--ts-checker` with node off PATH emits a plain syntax stream and says nothing. The reason is `tracing::info!("ts checker tier off: {err}")` at `src/project.rs:643`, silent by default. Rust: `tracing::warn!` at `src/project.rs:583`, and the "loaded a workspace containing NONE of the supplied files" case at `:594`. Under `--witness` a declined tier must be a `diagnostic` record in the stream itself.

Today: `load_ts_checker` (`src/project.rs:616`) and `load_rust_checker` (`:561`) return `Option<Index>`; the reason is dropped. `envelope` (`:479`) knows only `cx.indexes.ts_checker.get().is_some()` via `semantic_runs` (`:446`).

### Shape

```rust
// src/project.rs
/// Why a requested checker tier answered nothing. `None` = the tier was not
/// requested. Kept on the cx so the envelope can file it; off --witness the
/// string is built and dropped, which is cheaper than a second code path.
struct TierDecline { lang: &'static str, tool: &'static str, detail: String }
// ProjectCx (or a local beside it in resolve_project): declines: Vec<TierDecline>
// load_ts_checker / load_rust_checker: -> Result<Index, String>   (Err = the Display of the checker error, or the NONE-of-the-supplied-files sentence at :596)
// resolve_project :338/:348: on Err(detail) push TierDecline { lang, tool: "tsc" | "rust-analyzer", detail } and keep the tracing line as it is.
// envelope: after the coverage rows,
//   for decline in declines: rows.push(FlatFact::Diagnostic(DiagnosticOut { run: SYNTAX_RUN, relation: format!("tier.{}", decline.tool), detail: decline.detail }))
```

`DiagnosticOut` is `src/tsi/types.rs:73` `{ run, relation, detail }`; the record is already in the wire (`FlatFact::Diagnostic`, `src/types.rs`), the reverse door (`--ingest`, `src/tsi/ingest.rs`) and the v7 loader (`v7/src/2_comptime/0c_extract_loader.pl:171` `decode_record(diagnostic, ...)`). Nothing new on the wire. `relation` values: `tier.tsc`, `tier.rust-analyzer`. The `detail` is the exact reason text; the ts one today reads `no node driver: node is not on PATH`.

### Receipts (row 1)

New `tests/104_tier_decline_diagnostic.rs`, SABOTAGE RECEIPT header stating the base sha and that on it `--witness --resolve --ts-checker <root>` with `PATH` set to a directory holding no `node` emits zero `record=diagnostic` rows.
- `ts_tier_off_path_is_a_diagnostic`: run the binary with `PATH=<empty tmp dir>` (so `node` is absent), `--witness --resolve --family type --project-root tests/fixtures/tsi --ts-checker tests/fixtures/tsi tests/fixtures/tsi/probe.ts`; assert exactly one diagnostic with `run == 0`, `relation == "tier.tsc"`, `detail` contains `node`; assert no `run` row with `mode == "semantic"`.
- `rust_tier_off_is_a_diagnostic`: the rust analogue; pick the cheapest decline you can force (a `--rust-checker` root with no `Cargo.toml` is one); `relation == "tier.rust-analyzer"`.
- `no_witness_emits_no_diagnostic`: same args without `--witness`; the stream is byte-identical to today's (compare against the same command on the base sha, or assert zero `record=diagnostic`).
- `a_loaded_tier_files_no_decline`: under `#[cfg(feature = "ts-checker")]` with `SPREFA_TS_CHECKER_TYPESCRIPT` and node on PATH (copy the helper from `tests/98_resolve_witness.rs:183` `typescript()`), zero `tier.*` diagnostics.
`--ingest` over the new stream rc=0 (the `diagnostic` record already round-trips; `tests/97_ingest.rs` shows the shape).

## Row 2: `extract_resolve_carries_syntax_tsi_rows`

`v6/prolog/ARCH.pl:998`. A4's syntax-tier tsi rows (`bundle.aux.tsi`, `src/types.rs:412`, written by `tsi_rows` in `ts.rs` and the rust twin) ride the per-file `--family` stream only (`src/wire.rs:292-305`, digest rewrite under `if let Some(digest)`). `--witness --resolve` emits run 0 with `resolved_type_edge` rows and zero `tsi.*` facts, so with the checker declined the stream has no type graph at all.

### Shape

In `resolve_project` under `request.witness`, before `envelope`: for every `input` in `inputs` (path order, the order `flatten_inputs` fixed), for every row in `input.output.types.as_ref().map(|b| &b.aux.tsi)`, clone the row, rewrite every `Arg::Span(blob, _, _)` blob to `input.blob.to_string()` exactly as `wire.rs:297-303` does, and push `FlatFact::Fact(row)`. Lift that rewrite into one fn both call sites use (`src/wire.rs` or `src/tsi/mod.rs`, your call, one definition). The rows take `fact` ordinals through the existing `fact_slot()` loop in `envelope`, so put them into `facts` before `envelope` runs, after the resolved rows (numbering the resolved rows first keeps `tests/98_resolve_witness.rs` ordinal assertions stable; verify that, and if the goldens there move, say why in the PR body).

The `coverage` rows: `named_relations` in `src/wire.rs:148-160` already folds `aux.tsi` relation names for the per-file stream; the resolve envelope's coverage loop at `src/project.rs:545` names only `extract.call`/`extract.type`. Add one `partial` coverage row per distinct tsi relation the syntax run emitted, run 0, same ordering rule as the per-file path.

### Receipts (row 2)

New `tests/105_resolve_syntax_tsi.rs`, SABOTAGE RECEIPT header: on the base sha `--witness --resolve --family type --project-root tests/fixtures/tsi tests/fixtures/tsi/probe.ts` (no checker flag) emits zero `tsi.*` facts.
- `resolve_carries_the_syntax_tsi_rows`: the fact set of relation `tsi.*` from `--witness --resolve` over `probe.ts` equals, as a set of (relation, args) with span blobs compared, the set from `--witness --family type tests/fixtures/tsi/probe.ts`. Same for `probe.rs`.
- `every_syntax_tsi_fact_carries_an_ordinal`: every `record=fact` row has `fact` set and the ordinals are dense 1..n.
- `syntax_tsi_coverage_is_partial`: one `coverage` row per tsi relation emitted, `run 0`, `partial`.
- `the_resolve_stream_survives_the_reverse_door`: `--ingest` over the stream rc=0 with the declaring positions unchanged (`tests/97_ingest.rs`).
`tests/98_resolve_witness.rs` and `tests/100_tsi_intersection.rs` stay green; if an ordinal there moves, the reason is in the PR body.

## Gate (both rows)

`cargo test --test 96_witness_wire --test 97_ingest --test 98_resolve_witness --test 99_syntax_tsi_rows --test 100_tsi_intersection --test 104_tier_decline_diagnostic --test 105_resolve_syntax_tsi` green, and with `--features ts-checker` under the node/TypeScript env of `tests/98_resolve_witness.rs`. Then the full battery in the background (10-second law), `tail -30` pasted. `git diff --stat origin/main...HEAD` shows no golden outside the two new tests; the flag-off streams are byte-identical.

## Ownership

Owned: `src/project.rs`, `src/tsi/**`, `src/wire.rs` (only the lifted rewrite fn and `named_relations` if shared), `tests/104_*.rs`, `tests/105_*.rs`, `tests/98_resolve_witness.rs` and `tests/100_tsi_intersection.rs` only for an ordinal receipt that has to move.
Forbidden: `src/dispatch.rs`, `src/lang/**`, `tests/31_tracing.rs`, `v7/**`, `docs/failure-modes.md`, `v6/prolog/ARCH.pl`. Two other lanes own those.

## Style laws

No em dashes. Comments state only constraints the code cannot show. `tracing` only. Descriptive names. Banned words: provenance, substrate, load-bearing, regime, refusal, "ground truth". Commit subjects: `extract: a declined checker tier is a diagnostic record`, `extract: --resolve carries the syntax tier's tsi rows`.

## Done

Push, PR against `main` with receipts, then:
`boop beep --no-wait --as fix-extract-tier-diagnostic sprefa-coordinator "tier-diagnostic PR #<n>: 104 <n>/<n>, 105 <n>/<n>, 96-100 green, battery <pass>/<total>"`.
