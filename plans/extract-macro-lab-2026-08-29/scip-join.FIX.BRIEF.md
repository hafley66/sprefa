# Brief: macro-span call sites from the scip index (lane `feature-extract-rust-scip-macros`)

Read `plans/extract-corpus-2026-08-28/COMMON.md` and
`plans/extract-macro-lab-2026-08-29/PLAN.md` Option 4. User decision
(2026-08-29): options 1 (mbe, lane `feature-extract-rust-mbe`, running in
parallel) and 4 (this lane) BOTH land; the coordinator diffs them on the same
corpus afterwards. Your receipt must make that diff possible.

## First action
```
git merge --ff-only 9928476ff35ea361ef057f5bf300a866d0e70edc
cd v6/sprefa-extract && cargo build --release --features cli 2>&1 | tail -1
```
Failure: STOP, `boop beep --no-wait --as feature-extract-rust-scip-macros sprefa-coordinator "<one line>"`.
Binary: `v6/sprefa-extract/target/release/extract` in YOUR worktree.

## Ownership
Yours: new `src/lang/rust_scip_macros.rs`, `src/project.rs` (one post-pass
hook in `resolve_project`), `src/scip_rows.rs` if a reader helper is
missing, `tests/59_rust_scip_macros.rs`, `tests/fixtures/rust_findings/scip_macros/**`.
Forbidden: `src/lang/rust.rs` (two other lanes own it), `src/lang/rust_mbe.rs`,
`src/types.rs` beyond ONE additive `CallEdgeKind::ScipMacro` variant in its
own commit (plus the `_` arms in `tests/golden_parity.rs`), `src/lang/go*.rs`,
`src/lang/ts*.rs`.

## What exists
- `Resolve<CallF> for RustSource` binds only sites the parse arm minted; a
  call written inside a macro invocation has no site, so scip's exact
  occurrence for it (17,568 on rust-analyzer, PLAN.md Option 4) mints nothing.
- The parse arm emits every macro invocation's span (grep `macro_invocation`
  / `MacroCall` in `src/lang/rust.rs` and `tests/24_rust_specifiers.rs`;
  if no row carries the span today, read it from the syn parse in your own
  module, never edit rust.rs).
- `join_documents` / `scip_call_target` (`rust.rs`, `go.rs`) show how a
  scip occurrence is matched to a corpus def by symbol. Copy the shape into
  your module; the dedup sweep is a later arc.

## Build
1. Post-pass in `resolve_project` after the per-file `Resolve<CallF>`: for
   every rust file with a scip document, every scip call occurrence whose
   range lies inside a macro invocation span and matches NO existing site
   mints a `resolved_edge` caller=covering def, callee=scip target, kind
   `scip_macro`, call_site=the occurrence range.
2. One `macro_site` aux row per minted edge `{span, macro_name, source: scip}`
   so the coordinator can diff against mbe's `macro_site` rows by span.
3. Without a scip index the pass is a no-op and emits nothing.

## Tests, fail-first, commit per step
`tests/59_rust_scip_macros.rs`: a fixture crate with a local `macro_rules`
that calls `helper()` twice, indexed with `--scip-build`: two `scip_macro`
edges, both call_site spans inside the invocation; no scip index -> zero
rows; an occurrence that already has a parse site mints no duplicate.
COUNT: one document join per file, never per occurrence.

## Receipt
Rerun `plans/extract-crawl-2026-08-29/rust.crawl.py` over
`/Users/chrishafley/projects/rust-analyzer` with YOUR binary and the scip
index the lab built (`PLAN.md` Option 4 states the path and rustup
toolchain): `scip_macro` edges n, reachable union 12,221 -> n, and the
per-file top 10 gained. Write `macro_site` spans to
`plans/extract-macro-lab-2026-08-29/scip.macro_sites.tsv` (path, start,
end, macro_name). Append section 15 to `rust.REPORT.md`. Gate in
background, SUM. Push, `gh pr create --base main`, hail
`boop beep --no-wait --as feature-extract-rust-scip-macros sprefa-coordinator "rust scip macros: PR #N, scip_macro <n>, union 12,221-><n>, gate <p>/<f>"`.

## Laws (inline)
No em dashes. No `eprintln!`. Comments state constraints only, no dates.
Descriptive names. Every `extract` call under `timeout 10`. No `cargo fmt`
outside files you own. Never `--no-verify`.
