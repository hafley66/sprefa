---
created: 2026-08-27
updated: 2026-08-27
type: task
assignee: chris
status: open
priority: low
epic: extract-move-parity
labels: [extract, refactor]
---

# extract rename: plan doc before any code

## Description

v1 `DeclChange::Rename` / `plan_decl_rename`. The parity plan
(`plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md:39`, rank 5) says a
rename needs the resolved edge plane (`Resolve<F>`). Plan first, no
implementation.

Plan: `plans/2026-08-27-extract-rename.PLAN.md`.
Visual: `plans/2026-08-27-extract-rename.PLAN.visual.human.unga.md`.
Receipt: two docs, no code.

## Finding

The rank-5 row's premise does not survive measurement. `Resolve<F>` cannot
drive a rename, for three reasons the plan proves with `path:line`:

1. The reference-carrying rows carry no reference span. `TypeEdgeCandidate`
   (`types.rs:325`) and `TypeSig` (`types.rs:264`) name the referenced type as
   TEXT with no position; `CallSite` (`types.rs:453`) keeps the whole callee
   expression; `Specifier` (`types.rs:521`) keeps the whole import clause.
2. `ProjectCx` has no file set. `FileSet` and `ManifestMap` (`types.rs:1415`,
   `:1417`) are unit structs, constructed empty at `project.rs:160-161`, so a
   resolve arm cannot enumerate referencing files.
3. TypeScript's local `export {foo}` has no phase-1 row at all
   (`ts.rs:1239-1241`), and that form was v1's primary rename anchor.

Only two seats in the extractor are identifier-exact today: `rust.rs:266-292`
(TypeF item names) and `rust.rs:1533` (method-call idents). Nothing in
TypeScript is.

The plan therefore puts the rename on a sibling `Rename` trait over
`oxc_semantic` (+1 crate in this lock, every dependency already present), with
`Resolve<F>` and SCIP kept as an optional verify leg.

## Acceptance Criteria

- [x] `plans/2026-08-27-extract-rename.PLAN.md` lands with receipts and
      `path:line` citations for every claim.
- [x] `plans/2026-08-27-extract-rename.PLAN.visual.human.unga.md` lands with
      plain words, mermaid, and zero citations.
- [x] Prior art settled: v1 `plan_decl_rename` traced arm by arm; v5's absence
      proved by a zero-hit grep.
- [x] The edge plane a rename needs is settled, with a per-language table of
      def-site source, ref-site source, and whether spans are exact enough to
      Replace.
- [x] SCIP named as the second source, with `OccurrenceRole` bits and the
      line/col-to-byte bridge cited, and the reason it verifies rather than
      plans.
- [x] Trait shape written to the planning protocol: signatures, pseudo-code
      under each, lifetimes, storage plus read/write order plus uniqueness.
- [x] Sibling trait vs methods on `Rehome`: both shapes given, one
      recommended, reason stated.
- [x] Scope fence written: what a rename never rewrites, and the
      `--text-refs`-shaped report that covers each hole.
- [x] Arcs ranked smallest first, first arc is TypeScript on oxc judged
      byte-exact against a hand-written fixture.
- [x] Build-vs-buy analysis written candidate by candidate before any bespoke
      code is proposed.
- [x] No Rust changed. No file owned by the six move lanes touched.

## Tests Run

None. This issue's deliverable is two documents; no code changed, so no
suite ran. The `cargo add --dry-run` and registry reads used to price
`oxc_semantic` are recorded in the plan's Receipts section.

## Implementation Notes

Implementation is deliberately not dispatched by this issue. The plan's arc
table is the dispatch list, and arc 1 (trait, context, roster, CLI,
TypeScript restricted to one file) is the only arc ready to brief as written.

Arcs 1 through 4 and 6 touch no file the six concurrent move lanes own. Arc 5
reads two `pub(crate)` helpers in `lang/rust.rs` and must not edit
`lang/rust_rehome.rs` or `tests/3_move_rust.rs`.

## Agent Runs

### 2026-08-27T04:40:39Z · @move-symbol-rename-plan

Plan lane on `task/move-symbol-rename-plan`, base `afa481059`.

- Read `CLAUDE.md`, `AGENTS.md`, both move plans, and PR #489 before writing.
- Measured the resolved edge plane against the rename bar rather than assuming
  the rank-5 row: `Resolve<F>` cannot spell a rename, and the three reasons are
  cited in the plan's "Three structural gaps".
- Priced `oxc_semantic@0.135` with `cargo add --dry-run` plus a read of its
  vendored `Cargo.toml.orig`: 14 dependencies, all 14 already in this
  `Cargo.lock`, so the cost is +1 crate and one feature flip
  (`oxc_allocator/bitset`).
- Verified the oxc_semantic API against the vendored 0.135 source, not memory:
  `Scoping::symbol_span` (`scoping.rs:335`), `get_resolved_references`
  (`:551`), `Reference::node_id` (`oxc_syntax .../reference.rs:254`).
- Rendered all three mermaid boards through `mermaid-cli` 11. Shape counts
  16 / 9 / 6, all under the 24 budget; every board wider than tall.
- Zero Rust changed. Zero files owned by the six move lanes touched.

Deliverables: `plans/2026-08-27-extract-rename.PLAN.md`,
`plans/2026-08-27-extract-rename.PLAN.visual.human.unga.md`.
