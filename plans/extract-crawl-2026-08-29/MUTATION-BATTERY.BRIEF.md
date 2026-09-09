# Lane `test/extract-mutation-battery` (glm53): resolver invariants under mutation

First action: `git merge --ff-only 681c9126c18c7d59898ba7549585d30f1f0e0b4d`. Failure = STOP, beep the coordinator.

## Goal, one sentence

A test battery that MUTATES fixtures and asserts resolver invariants — a
duplicate def flips corpus-unique edges to absent (never to a wrong dst), a
relocated def keeps its edges (paths follow), an injected shadow drops the
edge — using the `resolution_origin` field (#619) to assert per-origin deltas.

## Why

Plan `plans/extract-eval-2026-08-31/PLAN.md` sec 5 (arc D), user word
2026-08-31: "i clearly want more testing i really am worried we are
overfitting". These properties hold by DESIGN claims; nothing tests them.

## Invariants (each one test fn, each per language where the fixture exists)

1. duplicate-def: copy a def named N from the fixture into a NEW file in the
   same temp corpus; every edge to N whose `resolution_origin` was
   `corpus_unique` must now be ABSENT. Present-with-different-dst = FAILURE
   (that is a guess; the whole point).
2. relocation: move a def to another file in the temp corpus (text move, both
   files rewritten); the edge count is unchanged and only dst_path moved.
3. shadow: insert a same-named parameter into the enclosing def above a call
   site (python fixture); the edge must drop or re-point to the param binding
   per the shadowing rule, never survive pointing at the module-level def.
4. origin conservation: for EVERY mutation, edges whose origin was `same_file`
   or `checker` in the base run are untouched (count and rows identical).

## Mechanics

- One new test binary `v6/sprefa-extract/tests/90_mutation_battery.rs`.
- Fixtures: copy existing dirs into a per-test tempdir (`tempfile` crate is
  already a dev-dependency — verify in Cargo.toml, cite; if absent, add
  dev-dep only), apply the mutation as string edits, run `resolve_project`
  in-process on before and after file sets, diff `ProjectEdge` rows.
- Source fixtures to reuse (read-only): `tests/fixtures/py_findings/`,
  `tests/fixtures/go_type_refs/`, `tests/fixtures/rust_findings/impl_owner/`,
  one ts fixture from `tests/fixtures/` (pick one with a corpus-unique edge,
  cite it).
- The battery runs under plain `cargo test --features cli`; each test under
  2 s (10-second law); no ratchet, no corpus dirs, no release build.

## Files you own

- `v6/sprefa-extract/tests/90_mutation_battery.rs` (new)
- `v6/sprefa-extract/Cargo.toml` ONLY if a dev-dep is genuinely missing
- FORBIDDEN: src/** (a failing invariant is a FINDING, never a fix — report
  it, leave the test red-ignored with `#[ignore]` + a comment naming the
  defect, beep the coordinator immediately), tests/bench/**, RATCHET.tsv,
  existing fixture files (copy, never edit).

## Receipts (PR body)

1. Table: invariant x language -> pass/fail/skipped-no-fixture.
2. Any failing invariant: the exact mutated input, the wrong edge row, and
   the origin it carried — as a finding, not a fix.
3. Suite `cargo test --features cli,rust-checker` rc=0 direct (whole suite
   still green with the new binary).
4. `git diff --stat origin/main...HEAD` = owned files only.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Comment budget: constraints only.
Commit per logical step; push; `gh pr create`; then
`boop beep --no-wait --as test-extract-mutation-battery sprefa-coordinator "<PR number, invariant table>"`.
