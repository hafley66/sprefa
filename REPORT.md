# lab/emit-wave REPORT

Two commits on base 3d97fd4f: 34a42f89 (arms run in arrival order, ruling
one_pick_order) and b3a8763d (level plane runs to fixpoint, fold wall down).

## Commit 1: one_pick_order
arrival_trigger_kind/4 (lower.pl ~1565) replaces the inline PreAtoms test:
ordered_arrival when the arm carries pre(Atom) OR a not(Atom) whose ref heads
an edge rule; a negation over a rel no edge rule writes stays arrival and its
emitted text is unchanged. One existing module across 203 flipped door
(one_attempt_guard_by_negation...winner.ts, runIncrementalTick ->
runOrderedTick); oracle untouched. Fail-first fixture
one_attempt_guard_by_negation_arrival_order_beats_arm_order (same arms, same
source order, arrivals reversed) proven WRONG pre-fix in sweep, identical
after.

## Commit 2: fold wall
recompute_levels_fn_lines/3 third clause: DELETE once, INSERT rounds under rx
expand until a round adds no row (count via ISqlRunner.scalar; INSERT OR
IGNORE + no inter-round DELETE = monotone count over finite store =
termination). Only self-read level heads get the loop (strat.pl already
refuses wider cycles), so 204 of 205 modules stayed byte-identical; the one
mover is flagship_flow_reach_over_resolved_edges.ts, the corpus's only
self-read head. Twin fixtures ordered_/unordered_program_level_fold_
reaches_three_links landed with verbatim fail-first receipts; the naive door
had the same wall and the same edit closed it (measured both modes).

## Gates
After each commit, lane runs + coordinator re-runs agree. Final: conformance
292 PASS/0 FAIL, plunit 324/324, TEXT_DOOR 205/205/0, tsv2 142 pass/2 skip,
sweep RUN 205 identical=204 wrong=0 (1 known rejection), golden-flex HOLDS
(all four perturbations + served e2e), typecheck/import-gate/prolog-lint 0.

## Named residues (out of brief, none fixed)
1. Mode parity is VACUOUS for ordered programs (EMITTER_MODE declared, never
   read, emit_ts.pl ~2088); commit 1 widens the vacuous set to legacy
   negation-guard programs. Honest fix = a ruling (drop the leg's claim or
   build a naive ordered twin).
2. Boot has the same one-pass shape (lower.pl boot_level_recompute_
   statements/2): a self-read head over seeded initial rows still stops at
   clause count AT BOOT. No corpus fixture in that shape.
3. departure_trigger still picks on PreAtoms alone: a finalize/1 arm negating
   an edge-headed ref keeps arm-major order (separate call, resolver shape
   differs).
4. arrival_trigger_kind/4 fires on shape, not on proven races: a single-arm
   program with an edge-headed negation takes the ordered door for no gain
   (zero corpus fixtures; cost unmeasured).
5. one(Positions) decl still does not exist; this closes one_pick_order for
   legacy programs; the decl needs one more clause in arrival_trigger_kind/4.
