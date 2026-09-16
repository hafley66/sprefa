# Brief: SCIP-verified import refs for `extract move` (new issue under epic extract-move-parity)

Read `CLAUDE.md` and `AGENTS.md` in full first, then `plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md`, `plans/2026-08-26-extract-move-rehome-trait.PLAN.md`, and PR #489 (`gh pr view 489`). User decision (Chris): every language is its own impl; no `match`/`if` on language anywhere in the move core (`src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`). Those three files plus `src/types.rs` Rehome trait, `src/lang/rust_rehome.rs`, `tests/1_move.rs`, `tests/3_move_rust.rs` are FORBIDDEN: six shootout lanes own them right now. If you need a change there, STOP and hail with the exact line. Style: comment budget = constraints only; banned words provenance, substrate, load-bearing, regime, refusal, ground truth; descriptive identifiers; `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test. Delivery: one PR against `origin/main`, do not merge, hail on post and on block: `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts>"`.

## First action
```bash
git merge --ff-only afa481059   # STOP AND REPORT on failure
issuectl new -t feature --slug move-scip-verify --title "extract move: verify Rehome import refs against a SCIP index" -a chris -e extract-move-parity -p normal -l extract -l refactor -l scip --description "When a fresh SCIP index exists, cross-check every ImportRef a Rehome impl produced against SCIP Occurrence rows with the IMPORT role; report refs SCIP knows that the impl missed and refs the impl produced that SCIP does not know. Library + tests first, no CLI flag (0_move.rs is owned by other lanes)."
```
Commit the issue in your PR.

## Files you own
- new `v6/sprefa-extract/src/move_scip.rs` + `lib.rs` wiring
- new `tests/5_move_scip.rs`, fixtures under `tests/fixtures/scip_move/**`
- `issues/move-scip-verify/item.md`

## Seams to read, not edit
- `src/types.rs:1887 ScipSource` (`indexer`, `build(root)`, `load(index_path)`), `OccurrenceRole::IMPORT`, `PositionEncoding`
- `src/scip.rs:76-200` impls (`ScipTypescript`, `ScipRust`, ...), `src/scip_decode.rs`, `src/scip_rows.rs`, `src/scip_ensure.rs`
- `src/types.rs` `Rehome::import_refs`, `ImportRef` (importer, literal span, target, kind, literal text)
- `src/move_cx.rs` `MoveCx::open` (read only; construct one in tests)

## What to build
```rust
pub struct ScipDisagreement { pub importer: String, pub span: Span, pub kind: &'static str /* "missed_by_impl" | "unknown_to_scip" */, pub detail: String }
pub fn verify_import_refs(cx: &MoveCx, index: &ScipIndex, refs: &[ImportRef]) -> Vec<ScipDisagreement>;
```
- Map each SCIP Occurrence with role IMPORT to (document path, byte range) using the document's PositionEncoding (UTF-16 columns -> bytes; do this right, test it on a non-ASCII line).
- A ref matches an occurrence when importer paths agree and the occurrence range overlaps the ref's literal span. One-to-one; leftovers on either side are disagreements, sorted by (importer, span).
- Never call `build`; the caller supplies a loaded index. Never touch the network.

## Fail-first tests
1. TS fixture indexed by a checked-in tiny `.scip` file (build it once with scip-typescript on the fixture, commit the binary, record the indexer version in the fixture README): zero disagreements against `TsSource::import_refs`.
2. Drop one ref from the impl output: exactly one `missed_by_impl`.
3. Add a fake ref: exactly one `unknown_to_scip`.
4. UTF-16 column mapping on a line with a multibyte char before the import.
If scip-typescript is not installed, STOP and hail; do not hand-write a .scip.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli`: full battery 0 failures; `5_move_scip` 4.
- `git diff afa481059 --stat` shows only owned files.
PR title: `extract move: SCIP-verified import refs (library + tests)`.
