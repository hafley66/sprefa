# Lane `fix/extract-py-mutation-findings` (glm53): F1 param shadow, F5 same-file duplicate

First action: `git merge --ff-only f03dc59e3bd165c7a99d4225be5a4f5d7004d647`. Failure = STOP, beep the coordinator.

## Goal, one sentence

Turn mutation-battery findings F1 and F5 green: a python parameter shadow must
beat the corpus name match, and a same-file duplicate def must drop the
corpus_unique edge instead of keeping it — un-ignore the two battery tests,
fix the resolver, PyCG precision stays 100.00%.

## The findings (tests/90_mutation_battery.rs, PR #622, both `#[ignore]` with the defect named in-tree)

- F1: `def consume(imported_fn): return imported_fn()` still emits
  `callee_path=helper.py origin=corpus_unique` — the param leg
  (`src/lang/python/_0_source.rs` `param_target` / `shadowed`) exists but the
  corpus name match runs FIRST for this shape. Find the ordering in
  `resolve_site` (`_0_source.rs:~2797`) and make the enclosing-def param win.
- F5: a same-file duplicate def keeps the corpus_unique edge (cross-file
  already flips to absent). The uniqueness check counts corpus files, misses
  same-file duplicate spans. Fix where `call_name_match` (or its DefIndex
  join) decides uniqueness.

## Method

1. Un-ignore the two tests, run the battery: both must be RED for the stated
   reason (paste the red output — that is your fail-first receipt).
2. Fix in `src/lang/python/_0_source.rs` (and `types.rs` ONLY if the DefIndex
   join itself under-counts and the fix is small — say so in the commit).
3. Battery green (all 4 langs, no other row changed: origin conservation
   tests are your bystander guard).
4. PyCG rescore: run the suite scorer exactly as
   `plans/extract-bench-2026-08-29/python-oracle/` does (SCORES.tsv
   regenerated); recall must not DROP below 69.49% and precision must hold
   100.00% (floor 99.03%). Remember SCORES.tsv has CATEGORY: rollup rows —
   filter them from any sum.
5. Suite `cargo test --features cli,rust-checker,ts-checker` rc=0 direct.

## Files you own

- `v6/sprefa-extract/src/lang/python/_0_source.rs`
- `v6/sprefa-extract/tests/90_mutation_battery.rs` (un-ignore + assert flips
  only; never weaken an assertion)
- `plans/extract-bench-2026-08-29/python-oracle/` regenerated tsvs
- `plans/extract-bench-2026-08-29/OPEN-PROBLEMS.md` row 2 if numbers move
- FORBIDDEN: rust/go/ts lang files, tests/bench/**, RATCHET.tsv, src/types.rs
  unless mechanism-forced small (state why in the commit).

## Receipts (PR body)

1. Red output of both un-ignored tests pre-fix; green battery post-fix.
2. PyCG table: recall/precision before -> after (filter CATEGORY: rows).
3. Suite rc=0 direct.
4. diff-stat = owned files only.

## Laws

Banned words prose+identifiers: provenance, substrate, load-bearing, regime,
ground truth (say oracle), honest, signal. Commit per finding; push;
`gh pr create`; then
`boop beep --no-wait --as fix-extract-py-mutation-findings sprefa-coordinator "<PR number, PyCG before->after>"`.
No ratchet run (python has no ratchet leg; PyCG rescore is the gate).
