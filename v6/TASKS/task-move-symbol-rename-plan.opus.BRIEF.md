# Brief: plan doc for `extract rename` (issue move-symbol-rename-plan, rank 5). NO CODE.

Read `CLAUDE.md` and `AGENTS.md` in full first, then `plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md`, `plans/2026-08-26-extract-move-rehome-trait.PLAN.md`, and PR #489 (`gh pr view 489`). User decision (Chris): every language is its own impl; no `match`/`if` on language anywhere in the move core (`src/0_move.rs`, `src/move_cx.rs`, `src/move_stage.rs`). Those three files plus `src/types.rs` Rehome trait, `src/lang/rust_rehome.rs`, `tests/1_move.rs`, `tests/3_move_rust.rs` are FORBIDDEN: six shootout lanes own them right now. If you need a change there, STOP and hail with the exact line. Style: comment budget = constraints only; banned words provenance, substrate, load-bearing, regime, refusal, ground truth; descriptive identifiers; `cargo fmt`; no `eprintln!` in `src/**`; 10-second law on every test. Delivery: one PR against `origin/main`, do not merge, hail on post and on block: `boop beep --no-wait --as <your-lane-name> sprefa-coordinator "<PR#, test counts>"`.

## First action
```bash
git merge --ff-only afa481059   # STOP AND REPORT on failure
```

## Files you own
- new `plans/2026-08-27-extract-rename.PLAN.md` (receipts, citations, for the auditor)
- new `plans/2026-08-27-extract-rename.PLAN.visual.human.unga.md` (plain words, mermaid, zero citations, for Chris; a plan without it is undelivered)
- `issues/move-symbol-rename-plan/item.md`: tick AC, Agent Runs note
Nothing else. No Rust.

## What the plan must settle, with `path:line` receipts
1. Prior art: v1 `~/projects/sprefa-archive-20260428/crates/watch/src/plan.rs:107 plan_decl_rename`, `crates/watch/src/diff.rs`, `crates/watch/src/change.rs:33`; v5 has none (cite the zero-hit greps in the parity plan).
2. The edge plane a rename needs: `Resolve<F>` (`types.rs:1663`), `ProjectEdge` (`:1290`), `FlatFact::Edge`, per language which impls exist (`lang/{ts,rust,kotlin,go,prolog,dl6,markdown}`), and `ScipSource` (`types.rs:1887`) occurrences with `OccurrenceRole::DEFINITION|IMPORT|READ_ACCESS|WRITE_ACCESS` as the second source. Table: language / def-site source / ref-site source / spans exact enough to Replace (yes/no, why).
3. Trait shape per the planning protocol (signatures first, pseudo-code under them, lifetimes, storage + read/write order + uniqueness). Propose `Rename` as a sibling trait to `Rehome` or as methods on it; give both, recommend one, say why in two lines.
4. Scope fence: what a rename never does (string literals, docs, reflection) and the `--text-refs`-style report for them.
5. Arcs with receipts, smallest first; the first arc must be TS on oxc, byte-exact against a hand rename in a fixture.
PR title: `plan: extract rename over the resolved edge plane (rank 5)`.
