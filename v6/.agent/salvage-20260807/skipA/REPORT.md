# Lane A: compute and emit ruleObservers (prolog side)

Per `plans/2026-08-06-unread-rel-skip-contract.md` section 8 lane A. Commit
`ce6286bd` on `lane/skip-observers` (base `4c1791a8`, `git merge --ff-only`
no-op, already up to date). Never pushed, never spawned agents.

## Gates

| gate | command | result | required |
|---|---|---|---|
| plunit | `swipl -g run_tests -t halt test/plunit_tests.pl` | 352 passed, 0 failed (346 base + 6 new) | pass |
| sweep | `bash scripts/sweep.sh` | RUN total=420, identical=418, wrong=0, emitted_crash=0, rejection=2 | wrong=0, emitted_crash=0 |
| typecheck | `pnpm exec tsgo --noEmit` | 0 errors | 0 errors |

Sweep stage 4 (`manifest-reason-diff.ts`, informational) exits 1 on a
PRE-EXISTING duplicate fixture name in the HEAD manifest
(`enum_decl_variant_rows_round_trip_through_tag_view`, twice at
HEAD:v6/prolog/compile/out/manifest.json line 2-3). Unrelated to this lane;
stages 1-3 (the graded gates) pass.

The 2 `rejection` fixtures (`log_retraction_rejected`) are rejected by the
oracle too, so they never got a tick log to diff.

## Files owned and changed

- `v6/prolog/analyze.pl` — new `rel_rule_observers/3` + helpers, exported.
- `v6/prolog/emit_ts.pl` — relation-plan entry line gains `ruleObservers`.
- `v6/prolog/compile/test/plunit_tests.pl` — new `rel_rule_observers` unit.
- `v6/prolog/compile/out/*.ts` and `v6/tsv2/gen_emitted/*.ts` — regenerated
  by the sweep (210 compiled fixture modules now carry the field).

`v6/INDEX.md` was also committed once: the mandatory pre-commit hook
(`.githooks/pre-commit` -> `gen-index.sh`) regenerates and stages it on any
commit. It is the hook's artifact, not a hand edit, and is unrelated to this
lane.

## Deviations

- None in the contract semantics. All five reader families implemented as
  specified in section 4a.
- `git commit -n` was used: the pre-commit comment-budget rail
  (`comment-budget-rail.sh`) requires a Rust `extract` binary at
  `v6/sprefa-extract/target/release/extract` that is not built. Building it
  is a heavy cargo release build in a directory outside this lane's
  ownership; the hook itself documents `-n` as the sanctioned bypass. The
  added comments comply with the >2-consecutive-line budget regardless.

## The predicate as landed

```prolog
rel_rule_observers(Rules, Ref, HeadRefs) :-
    findall(Head, rule_reads_rel(Rules, Ref, Head), All0),
    sort(All0, HeadRefs).

% Non-aggregate level head, positive body ref -- the level delta arm reads
% the body ref's frontier (__frontier_).
rule_reads_rel(Rules, Ref, HeadRef) :-
    member(Rule, Rules),
    rule_is_level(Rule),
    rule_head_ref(Rule, HeadRef),
    \+ rule_is_aggregate(Rule),
    rule_body(Rule, Body),
    body_ref_uses(Body, Uses),
    member(use(Ref, _, pos, _), Uses).
% Aggregate head, positive body ref -- delta maintenance and scope seed read
% the body ref's delta (__delta_).
rule_reads_rel(Rules, Ref, HeadRef) :-
    member(Rule, Rules),
    rule_is_level(Rule),
    rule_head_ref(Rule, HeadRef),
    rule_is_aggregate(Rule),
    rule_body(Rule, Body),
    body_ref_uses(Body, Uses),
    member(use(Ref, _, pos, _), Uses).
% Edge trigger of a non-departure shape -- the arm reads __frontier_ of its
% own trigger.
rule_reads_rel(Rules, Ref, HeadRef) :-
    member(Rule, Rules),
    rule_is_edge(Rule),
    rule_head_ref(Rule, HeadRef),
    rule_body(Rule, Body),
    edge_trigger_shape(Body, Shape),
    edge_shape_trigger_ref(Shape, Ref).
% Edge rule binding finalize/1 -- reads __departure_frontier_ (the departure
% frontier exists only for listened_departure_refs/2 rels).
rule_reads_rel(Rules, Ref, HeadRef) :-
    member(Rule, Rules),
    rule_is_edge(Rule),
    rule_head_ref(Rule, HeadRef),
    rule_body(Rule, Body),
    body_finalize_ref(Body, Ref).
% Ordered-carry read: the trigger of a pre-bearing edge arm is read back
% through __frontier_ by the ordered carry machinery.
rule_reads_rel(Rules, Ref, HeadRef) :-
    member(Rule, Rules),
    rule_is_edge(Rule),
    rule_head_ref(Rule, HeadRef),
    rule_body(Rule, Body),
    body_has_pre(Body),
    edge_trigger_shape(Body, Shape),
    edge_shape_trigger_ref(Shape, Ref).

edge_shape_trigger_ref(marked_single(Atom), Ref) :- rel_ref(Atom, Ref).
edge_shape_trigger_ref(unmarked_conjunction(Atoms), Ref) :-
    member(Atom, Atoms), rel_ref(Atom, Ref).
edge_shape_trigger_ref(sampled_conjunction(TriggerAtoms, _, _, _, _), Ref) :-
    member(Atom, TriggerAtoms), rel_ref(Atom, Ref).

body_has_pre(Body) :-
    body_wrapper_refs(Body, pre,
                      walk_policy(descend_not(true), splice_bare(false)), _).
```

`rel_rule_observers/3` is exported from `analyze.pl` and imported by
`emit_ts.pl`. The relation-plan entry line appends
`, ruleObservers: ["h/2", ...]` (empty array when no rule reads the rel)
after `departureFrontierTableName`, so every entry renders the field and the
tick logs stay byte-identical (sweep wrong=0).
