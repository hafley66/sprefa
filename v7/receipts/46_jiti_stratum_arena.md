# 46. JITI stratum arena

Date: 2026-09-11. Implementation worktree based on
`30e1285b834ac6189155601c3436f72cc5dd0700`. SWI-Prolog 10.0.2 arm64-darwin.
This receipt covers only the evaluator stratum collection described as the
smallest implementation slice in receipt 45. No other arena collection,
public phase term, language rule, type rule, binding rule, or phase boundary
changed.

## Signatures and storage

```prolog
open_stratum_arena(+Strata) is det.
install_stratum_facts(+Strata, +ArenaId) is det.
close_stratum_arena is det.
arena_stratum(?Relation, ?ArenaId, ?Level) is nondet.
relation_level(+IgnoredStrata, +Relation, -Level) is det.
```

`arena_stratum/3` is dynamic. `stratum_arena_scope/1` is thread-local and acts
as an innermost-first scope stack. `open_stratum_arena/1` obtains an explicit
`dl7_stratum_arena_*` id with `gensym/2`, pushes it with `asserta/1`, and walks
the input once:

```typescript
for (const stratum of Strata) {
  assertz(arena_stratum(stratum.relation, arenaId, stratum.level));
}
```

The recursive Prolog implementation uses `assertz/1`, so duplicate relation
facts retain source-list order. `relation_level/3` binds the current arena id,
queries `arena_stratum(Relation, ArenaId, DerivedLevel)` once through `->/2`,
and returns `0` when that query has no solution. The first asserted matching
fact therefore retains the prior `memberchk/2` first-match result.

## Lifecycle and isolation

`evaluate/4` still validates the ground rules and seeds, derives dependencies,
and stratifies first. It then owns both transient stores for the remainder of
that evaluation:

```prolog
setup_call_cleanup(
    open_lower_store,
    setup_call_cleanup(
        open_stratum_arena(Strata),
        evaluate_after_stratify(...),
        close_stratum_arena),
    close_lower_store).
```

`close_stratum_arena/0` pops the innermost thread-local id and retracts only
`arena_stratum(_, ArenaId, _)` clauses for that id. Nested evaluations retain
the outer scope and facts while the inner evaluation is active. Simultaneous
threads share the dynamic predicate but use different ids and different
thread-local current scopes. The setup predicate also catches failure or an
exception during partial installation, removes the partially asserted facts
and scope, and then preserves the original failure or exception.

Focused residue checks observed zero arena facts and zero scope rows after
success, goal failure, goal exception, and partial setup failure.

## Realized JITI index

A fresh bounded process installed 200 ordered facts, queried all 200 relations,
and inspected both `predicate_property/2` and
`library(prolog_jiti):jiti_list/1` before cleanup. The realized non-list index
was:

```prolog
hash{
  arguments:[1],
  buckets:256,
  collisions:37,
  list:false,
  position:[1],
  realised:true,
  size:13040,
  speedup:200.0
}
```

`jiti_list/1` reported `arena_stratum/3`, 200 clauses, index `1:1`, 256
buckets, speedup 200.0, and 37 collisions. Cleanup in the same process then
reported `facts=0` and `scopes=0`.

## Exact output parity

The nearest-shadow fixture is
`v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7`. Fresh-process output
was serialized with `write_canonical/1` over compiler rows and hashed with
SHA-256.

| state | rows | diagnostics | SHA-256 |
| --- | ---: | ---: | --- |
| before | 810 | 0 | `fc7c8e3723c9017ec4c09efd21cdb62b579619c0efa57d0fcfad410c493dfbe4` |
| after | 810 | 0 | `fc7c8e3723c9017ec4c09efd21cdb62b579619c0efa57d0fcfad410c493dfbe4` |

The focused parity test pins this hash. The existing performance gate also
compares the complete cold and warm `output(CompilerRows, RuntimeProgram,
Diagnostics)` terms with `==/2`; it passed with 132 runtime relations, 120
runtime rules, zero runtime seeds, and empty diagnostics.

## Cold measurements

Each row below is one fresh `swipl` process under `timeout 15`, with tracing
off during the measured compile. Processes ran sequentially. The before sample
was captured before the shared-worktree implementation edit. Charged
inferences are deterministic across the four after samples.

| state | sample | cold wall ms | cold inferences | rows | diagnostics |
| --- | ---: | ---: | ---: | ---: | ---: |
| before | 1 | 426 | 1,899,632 | 810 | 0 |
| after | 1 | 560 | 1,899,975 | 810 | 0 |
| after | 2 | 422 | 1,899,975 | 810 | 0 |
| after | 3 | 601 | 1,899,975 | 810 | 0 |
| after | 4 | 418 | 1,899,975 | 810 | 0 |

The charged-inference delta is `+343`. The four after wall samples span
418 to 601 ms, with median 491 ms. One before wall sample does not establish a
wall-time distribution. Every sample remained below the existing 3,000 ms
cold gate. Warm measurements remained 2,240 inferences and 4 to 5 ms.

An additional `DL7_TRACE=steps` fresh process completed in 444 ms and showed
the evaluator install, collect, and cleanup steps with nonzero table snapshots
where expected. Tracing raised the replay inference count to 1,964,354, so that
run is excluded from the trace-off comparison above.

## Scan counts

Receipt 45 measured 18,921 nearest-shadow calls through `relation_level/3`.
A post-change `library(prolog_wrap)` count reproduced 18,921 calls, now served
by `arena_stratum/3` rather than `memberchk/2`.

The source inventory moved from four to three `memberchk(stratum(...))` sites.
The three remaining sites are outside this slice. A fresh nearest-shadow
instrumented process counted their calls:

| remaining list-scan predicate | file | calls |
| --- | --- | ---: |
| `rule_at_level/3` | `v7/src/1_libtime/0_evaluator.pl` | 1,692 |
| `demand_cone_eligible_rule/4` | `v7/src/1_libtime/0_evaluator.pl` | 252 |
| `relation_stratum/3` | `v7/src/2_comptime/1_checker.pl` | 788 |
| total | | 2,732 |

No origin, reservation, pending-edge, relation, or evaluator-lower storage was
changed.

## Tests and CI coverage

Eight deterministic cases were added to `v7/test/1_entrypoints.test.pl`:
lifecycle success, level-zero default, duplicate first-match, failure and
exception cleanup, partial-open cleanup, nested isolation, simultaneous-thread
isolation, and nearest-shadow canonical parity. Each case ran in its own
`timeout 3` process. The maximum PLUnit case time was 0.625 s and the maximum
whole-process wall time was 0.88 s.

Ten existing demand-cone and evaluator integration cases were also run one at
a time. The maximum PLUnit case time was 1.122 s and the maximum whole-process
wall time was 1.24 s.

Coverage added: eight focused repository test cases. Coverage changed or
removed: zero cases. CI workflow definitions were unchanged. The current
`dl7-userland` workflow does not load `1_entrypoints.test.pl`; its existing
nearest-shadow compiler performance gate does execute the modified evaluator
path.
