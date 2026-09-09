# Lane `feat/extract-origin-column` (opus): resolution-origin on every edge + 3-bucket precision

First action: `git merge --ff-only e89b986c79122cb7e826a022b01e2ecca9d9cc5a`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Every resolved edge carries WHICH leg answered it (`resolution_origin`), the
ratchet prints per-origin counts per language, and the bench scorer splits
ours-only rows into matched / contradicted / unjudged (3-bucket precision) —
with the 4-column match protocol unchanged.

## Why (context for zero-context reader)

Plan `plans/extract-eval-2026-08-31/PLAN.md` sec 4 (arc C). Recall gains land
through many small resolver legs; today nothing says which leg produced which
edge, so a leg that starts over-answering is invisible until precision moves,
and a CodeQL disagreement is indistinguishable from a CodeQL blind spot.

## Design constraints (do these exactly)

- `resolution_origin` is a closed enum in `v6/sprefa-extract/src/types.rs`:
  `same_file | corpus_unique | module_plane | checker | alias_chain | param |
  decorator | subscript | scip`. A leg not on the list gets a new variant, never
  a string. Wire it through `ProjectEdge` and the jsonl output as one new
  field. NEVER a stringly-typed column (sql-relational-design law).
- The oracle scorers (`tests/bench/mod.rs`) MUST keep matching on the same 4
  columns; origin is carried, printed, aggregated, never joined on.
- 3-bucket: for each ours-only row, `contradicted` = oracle holds a row with
  the same (src_path, src_name) and a different dst; `unjudged` = oracle holds
  no row for that (src_path, src_name). Print per oracle:
  `matched / contradicted / unjudged` counts beside the existing precision.
- Per-origin counts print in the ratchet table per language. Floors and
  tolerances in RATCHET.tsv are UNCHANGED by this lane; if any recall or
  precision cell moves at all you introduced a defect, stop and find it.

## Files you own (disjoint ownership, coordinator-enforced)

- `v6/sprefa-extract/src/types.rs`, `src/project.rs`, `src/wire.rs`,
  `src/family.rs` (enum + plumbing only)
- `v6/sprefa-extract/src/lang/*.rs` (each resolve leg tags its origin; smallest
  possible diff per file)
- `v6/sprefa-extract/tests/bench/mod.rs`, `tests/ratchet_recall.rs`
- `v6/sprefa-extract/tests/91_origin_column.rs` (new; fail-first)
- `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` — the protocol-forks row
  only, updated in the same PR
- FORBIDDEN: RATCHET.tsv (no bump), corpus dirs, plans/** other than the one
  row, everything under `v6/prolog`, `v6/tsv2`, root `src/`.

## Receipts (all in the PR body)

1. Fail-first: `tests/91_origin_column.rs` red before the enum lands (an edge
   from a fixture asserts its expected origin), green after. Per-language: one
   fixture assertion each for ts, go, rust, python.
2. Suite: `cargo test --features cli,rust-checker` rc=0, direct rc capture, no
   pipes.
3. Gate: `cd v6 && just extract-ratchet` rc=0, all rows hold, and paste the new
   per-origin table for all three legs plus the 3-bucket line per oracle.
4. `git diff --stat origin/main...HEAD` listing only owned files.

## Laws in force

- tracing only, no eprintln in src/**.
- Comment budget: constraints only, no narrative.
- Banned words in prose AND identifiers: provenance, substrate, load-bearing,
  regime, ground truth (say oracle), honest, signal.
- Commit per logical step. Push and post the PR yourself
  (`gh pr create`), then:
  `boop beep --no-wait --as feat-extract-origin-column sprefa-coordinator "<PR number, gate numbers>"`.
- 10-second law: no foreground wait over 10 s; builds and gates run with
  `nice -n 15`, batteries in background. ONE ratchet on the machine at a time:
  before `just extract-ratchet`, beep the coordinator and wait for its go if
  another gate might be running.
