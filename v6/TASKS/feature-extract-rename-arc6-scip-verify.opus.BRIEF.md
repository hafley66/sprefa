# Brief: extract rename, arc 6: the SCIP second opinion

Read `CLAUDE.md` and `AGENTS.md` in full first. Then
`plans/2026-08-27-extract-rename.PLAN.md` in full (contract :269-521, ":180 SCIP as the second source", arcs table :556 row 6 is yours, receipts :577), the landed rename verb `src/0_rename.rs` (#511 #514 #516 #517), `src/scip.rs` (`ScipTypescript::load` :115, `byte_range` :491, the `build` subprocess arm), `src/types.rs:1901` (`ScipSource`), `:1736` (`ScipOccurrence`, roles at :1685-1692), and PR #496's finding: scip-typescript 0.4.0 NEVER sets the IMPORT role; an import clause occurrence carries only the symbol. User decision (Chris, 2026-08-27): SCIP is verify-only; it reports disagreements and never changes the plan.

## First action
```bash
git merge --ff-only ecc921795e7fd72c5a9c3d78d8690a8323b6693c   # STOP AND REPORT on failure
scip-typescript --version   # must print 0.4.0; STOP AND REPORT otherwise
```

## Files you own
- `v6/sprefa-extract/src/0_rename.rs` (the `--verify-scip <index.scip>` flag and its call), new `src/1_rename_verify.rs` (the whole leg; `0_rename.rs` gains one call line)
- `tests/4_rename_ts.rs` (append), new `tests/fixtures/ts_rename/exports/before/package.json` + `after/package.json` (identical; scip-typescript needs one) if the indexer refuses to run without it
- new issue: `issuectl new -t feature --slug extract-rename-arc6-scip-verify --title "extract rename: arc 6, the SCIP verify leg" -a chris -p normal -l extract -l rename --description "plans/2026-08-27-extract-rename.PLAN.md arc 6"`; tick it as its own commit AFTER the code commit.
FORBIDDEN: `src/types.rs`, `src/rename_cx.rs`, `src/2_move_text.rs`, `src/scip.rs`, `src/scip_*.rs`, `src/lang/**`, `src/0_move.rs`, `src/move_*.rs`, `tests/5_*`, `tests/6_kind_vocab.rs`, `tests/fixtures/kind_vocab/**`, `tests/fixtures/rust_rename/**`, everything under `v6/sprefa-engine-rs` and `v6/sprefa-store`. Never commit `Cargo.lock`. Never commit a `.scip` index; the test builds it into a temp dir.

## Shape (plan row 6, signatures first)
```rust
// 1_rename_verify.rs
pub struct ScipDisagreement { pub file: String, pub start: u32, pub end: u32, pub side: DisagreementSide }
pub enum DisagreementSide { PlanOnly /* a plan span no occurrence covers */, ScipOnly /* an occurrence of the symbol the plan missed */ }
/// Loads the index through `ScipSource::load`, maps every occurrence of the
/// anchor symbol with a DEFINITION, READ_ACCESS or WRITE_ACCESS role (IMPORT is
/// never set by scip-typescript 0.4.0, so an import-clause occurrence counts by
/// symbol alone) to bytes through `scip::byte_range`, and diffs both ways
/// against the plan's SymbolRef spans.
pub fn verify_against_scip(plan: &RenamePlan, index_path: &Path, root: &Path) -> Result<Vec<ScipDisagreement>, RenameStop>
```
- The anchor symbol is found by the occurrence with the DEFINITION role at the anchor's declaration span; if none, that is one `PlanOnly` row for the declaration and the leg stops there.
- Output: one line per disagreement, `scip-verify <file>:<start>-<end> plan-only|scip-only`, then `scip-verify disagreements=<n>`. Exit code unchanged by the count (the flag reports; it never changes the plan or the exit).
- Run order: after the plan is printed, before any write.

## Fail-first tests (append to `tests/4_rename_ts.rs`)
1. `scip_verify_agrees_on_the_exports_fixture`: build a fresh index (`scip-typescript index` in a copy of `exports/before/`, output to the temp dir), run the dry-run rename with `--verify-scip <index>`, assert `scip-verify disagreements=0` and zero `plan-only`/`scip-only` rows. `#[ignore]` with measured seconds if over 10 s (indexing is the named exception to the 10-second law, so `#[ignore]` is expected; run once by hand and paste the time).
2. `scip_verify_reports_a_missed_seat`: the same index against a plan run with `--at` pointed at a decoy declaration (arc 2's ambiguous fixture shape, copied into the temp tree) yields `scip-only` rows for every real seat and `plan-only` for the decoy's; count asserted.
3. `scip_verify_never_changes_the_plan`: with and without the flag, the plan lines and the committed tree are byte-identical.
Write each first, paste the failing line in the commit body, then implement.

## Receipts (PR body)
- `cargo test -p sprefa-extract --features cli` in the FOREGROUND (never background): 0 failures, count; `4_rename_ts` count (arc 4 had 13); the ignored test's measured time and its by-hand pass.
- The literal `scip-verify disagreements=0` line from the by-hand run on the exports fixture.
- `grep -n 'panic!\|unwrap()' src/0_rename.rs src/1_rename_verify.rs` = 0 lines.
- `git diff ecc921795e7fd72c5a9c3d78d8690a8323b6693c --stat`: only owned files; `cargo fmt`; no new `eprintln!`.

## Style
Comment budget: constraints only. Banned words: provenance, substrate, load-bearing, regime, refusal, ground truth, "support". Descriptive identifiers. Async banned; the indexer subprocess goes through the existing `scip.rs` build arm or `std::process::Command`, never a shell string. Issue tick as its own commit AFTER the code commit.

## Delivery
One PR against `origin/main`, title `extract rename: arc 6, the SCIP verify leg`. Do not merge. Hail on post and on block:
`boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts, disagreements line, index time>"`.
