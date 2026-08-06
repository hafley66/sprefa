# brief: subscribe rename + strict zero-query flip (ruled; assembly only)

Mechanical lane. If reality deviates, STOP and write REPORT.md.
Worktree /Users/chrishafley/projects/sprefa-lab-subrename, branch
lab/sub-rename at 719901f8. FIRST ACTION: `git log --oneline -1` must
show 719901f8; else STOP. Commit in two steps (rename, then flip), no
pushes, no subagents.

## Step A: rename (vocabulary ruling: subscribe, never demand)
- v6/prolog/2_demand_cone.pl -> v6/prolog/2_subscribe.pl; module name
  '2_subscribe'; predicate demand_cone/4 -> subscribed_rels/4; update
  every internal name containing demand to subscribe wording.
- v6/prolog/compile/test/2_demand_cone.plt ->
  v6/prolog/compile/test/2_subscribe.plt; test group demand_cone ->
  subscribe_cone; loader line in
  v6/prolog/compile/test/plunit_tests.pl updated to the new file name.
- Callers: v6/prolog/compile.pl (use_module + the demand_cone call +
  the DemandedRels variable -> SubscribedRels), v6/prolog/emit_ts.pl
  (demanded_rel_json -> subscribed_rel_json; emitted const demandedRels
  -> subscribedRels; interface field in IGenProgramWithBoot),
  v6/prolog/conformance/engine.pl (import + call),
  v6/tsv2/runtime/types.ts (IDemandedRel -> ISubscribedRel,
  demandedRels -> subscribedRels).
- grep -rn "demand_cone\|demandedRels\|DemandedRels\|IDemandedRel"
  across v6/prolog v6/tsv2/runtime must return ZERO after (gen_emitted/
  compile/out hits die at the step-B sweep). Do NOT touch
  __host_demand_* names (separate pre-existing family, explicitly not
  ruled).

## Step B: strict flip (ruling zero_query_semantics = subscribes_nothing)
In 2_subscribe.pl: the Queries == [] branch returns [] (a program with
no query subscribes to NOTHING). Update tests: zero_query_all_rels
becomes zero_query_subscribes_nothing expecting Cone == []; the compat
wording in comments becomes: strict per ruling zero_query_semantics
2026-08-03; harness-side subscription roots arrive with the pruning
rung. Check the other 11 tests: any that relied on the compat branch
get updated to explicit queries. golden_flex invariants unaffected
(golden-flex has queries).
Then regenerate: from v6 run `just sweep` (or the sweep recipe the
justfile names) and commit the gen churn: every zero-query module's
constant becomes []. Behavioral grading must stay clean.

## Gates (paste into REPORT.md)
just plunit (expect 319 total, 0 fail; renamed group visible),
just conformance (285 PASS / 0 FAIL), just text-door (199/199/0),
sweep RUN/FINAL lines with wrong=0, and the zero-grep receipt from
step A.

## Style
Banned words: provenance, substrate, load-bearing, regime. Comment
budget: constraints only. Two commits: A then B, messages naming the
rulings.

## AMENDMENT 2: the plunit total in the gate was stale at your base (sibling tests landed before you branched). Correct expectation: 321 total, 0 fail (323 after any tests step B adds). Everything else stands. Proceed: commit A, then step B, then gates.
