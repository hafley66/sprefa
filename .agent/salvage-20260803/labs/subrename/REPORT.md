# REPORT.md — subscribe rename + strict zero-query flip (COMPLETE)

**Lane:** ruled, assembly only. First action verified: `git log --oneline -1`
showed `719901f8`, matching the brief. Two commits made, no pushes.

## Commit A — rename (ruling: subscribe, never demand)

`4f7a57f9` subscribe-rename: PASS

- `v6/prolog/2_demand_cone.pl` -> `2_subscribe.pl`, module `'2_subscribe'`,
  `demand_cone/4` -> `subscribed_rels/4`, all internal demand naming ->
  subscribe.
- `compile/test/2_demand_cone.plt` -> `2_subscribe.plt`, group `demand_cone`
  -> `subscribe_cone`, loader line in `plunit_tests.pl` updated.
- Callers updated: `compile.pl` (use_module + call + DemandedRels ->
  SubscribedRels), `emit_ts.pl` (demanded_rel_json -> subscribed_rel_json,
  const demandedRels -> subscribedRels, IGenProgramWithBoot field),
  `conformance/engine.pl` (import + call), `lower.pl` (variable),
  `tsv2/runtime/types.ts` (IDemandedRel -> ISubscribedRel, demandedRels ->
  subscribedRels).
- `__host_demand_*` family untouched (the only `__host_demand` diff lines
  elsewhere are this lane's demandedRels field rename; the host function names
  are unchanged).

## Commit B — strict zero-query flip (ruling zero_query_semantics 2026-08-03)

`3e032b4d` subscribe-zero-query: PASS

- In `2_subscribe.pl` the `Queries == []` branch now returns `[]` (a program
  with no query subscribes to NOTHING); `program_rels/3`, `declared_rel/2`,
  `rule_rel/2` (the old compat value) deleted.
- Tests: `zero_query_all_rels` -> `zero_query_subscribes_nothing` expecting
  `Cone == []`; `decl_walk` test moved to an explicit query; the
  `declared_rels_match_analyze` parity test reworked as
  `declared_rels_do_not_leak_into_a_queryless_cone`; emitted zero-query test ->
  `zero_query_module_subscribes_nothing` expecting `[]`.
- `golden_flex` invariants unaffected (golden-flex.dl6 has queries).
- Sweep regenerated every emitted module: 195 zero-query modules now carry
  `subscribedRels = []`; only the 4 query-bearing modules carry a non-empty
  cone. Behavioral grading clean.

## Gates

| gate | result |
|---|---|
| `just plunit` | 321 total, 0 fail (exit 0; `subscribe_cone` group visible) |
| `just conformance` | 285 PASS / 0 FAIL (exit 0) |
| `just text-door` | compiled=199 byte_identical=199 failures=0 |
| `just sweep` | RUN total=199 wrong=0; FINAL total=199 wrong=0 (exit 0) |
| zero-grep receipt | `grep -rn "demand_cone\|demandedRels\|DemandedRels\|IDemandedRel" v6/prolog v6/tsv2/runtime` -> zero matches (exit 1), including generated `compile/out` and `gen_emitted` |

## Deviation notes

- The brief's gate listed `just plunit` -> 319. Actual base count was 321 (per
  AMENDMENT 2, stale sibling-test delta predates this lane). Final plunit: 321
  total, 0 fail. No new tests were net added in step B, so the total stayed
  321, matching the amendment's reaffirmed expectation.
- The first `just sweep` run failed at stage 3 with `ERR_MODULE_NOT_FOUND:
  rxjs`. Root cause: missing npm installs in `v6/tsv2` and the linked
  `v6/sprefa-store/js` (no `node_modules`). Fixed with `pnpm install` in both;
  `pnpm-lock.yaml` unchanged, no lockfile churn. Re-ran `just sweep` -> wrong=0.
- `git gc.log` warning / loose-object message surfaced during commit B; cosmetic
  GC housekeeping, not a correctness issue in this lane.

## Working tree

Clean except the lane docs `brief.md` and this `REPORT.md` (untracked, not
committed). Branch `lab/sub-rename`, no pushes.

## Ask / next

Nothing blocking. The lane is done; results above are ready for the merge
decision.
