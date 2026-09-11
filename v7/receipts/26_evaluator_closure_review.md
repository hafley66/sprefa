# Evaluator closure-root review

## Scope

Review of the `collect_closure` optimization candidate on base
`3405509c6`. The candidate changes only closure materialization:

```text
evaluate_strata/9
  -> evaluate_stratum_after_aggregates/14
       -> install_evaluation/5       unchanged rule and demand-cone inputs
       -> collect_closure/4          bound relation roots plus lower union
```

The source checkout was concurrently modified by the adjacent optimization
lane during this review. No source file was edited by this lane. The only
owned artifact is this receipt.

## Required signatures and wiring

The candidate needs these exact contracts:

```prolog
current_result_relations(+CurrentRules, +StratumSeeds, -Relations) is det.

collect_closure(+EvaluationId, +ResultRelations, +LowerRows, -Closure) is det.

evaluate_stratum_after_aggregates(
    +AggregateDiagnostics, +AggregateSeeds,
    +Level, +MaxStratum, +Strata, +Dependencies, +Rules, +Seeds,
    +LowerRows, +PlainRules, +CurrentRules, +CurrentSeeds,
    -Closure, -Diagnostics) is det.
```

`CurrentRules` must be passed from `evaluate_strata/9`. It cannot be recovered
from `PlainRules`: aggregate-headed rules are deliberately absent from
`PlainRules`, while their derived rows are installed as `AggregateSeeds` and
must be collected through their aggregate rule-head relation.

One intermediate candidate snapshot had the call site using
`CurrentRules` without a clause argument. That produced a singleton warning
and an unbounded `member/2` traversal. The call site and both
`evaluate_stratum_after_aggregates/14` clauses must agree on arity.

## Candidate closure law

Let:

```text
Roots = { ref(kernel(nil)) }
        ∪ heads(CurrentRules)
        ∪ relations(StratumSeeds)

CurrentRows = { Row |
               Relation ∈ Roots,
               proves(EvaluationId, call(Relation, _)) = Row }

Requests = all evaluation_request(EvaluationId, Request)
NewRows = ord_union(sort(CurrentRows), sort(Requests))
Closure = ord_union(LowerRows, NewRows)
```

The explicit `kernel(nil)` root is required. `proves/2` has an unconditional
`call(ref(kernel(nil)), [const([])])` clause, so a no-rule/no-seed program
still has one closure row.

The `ord_union/3` preconditions are part of the contract:

1. `Rules`, `Seeds`, and `LowerRows` are ground under `evaluate/4`.
2. `LowerRows` is an ordered set. The first stratum starts with `[]`; each
   later stratum receives the preceding `Closure`.
3. `CurrentRows` and `Requests` are sorted before union.
4. Every relation in `Roots` is ground and is passed to `proves/2` as
   `call(Relation, _)`, never as an unbound relation call.
5. Closure rows remain ground, unique, and ordered.

`profile_closure_rows(NewRows)` records new rows per stratum. This changes the
meaning of the profiler category from the complete per-stratum snapshot to
the materialized delta. The `stratum_closure_rows` metric still reports the
length of the complete `Closure` after the lower union.

## Scenario review

| Scenario | Required root or carried state | Result |
| --- | --- | --- |
| Same-stratum recursion | Every rule head in `CurrentRules`; tabled `proves/2` still receives the recursive relation as a bound root | Preserved. A `path` root enumerated the complete transitive closure in the focused evaluator case. |
| Lower positive dependency | Current head root, plus unchanged `PlainRules` demand cone and `LowerRows` | Preserved. A bound upper query still calls the lower definition. Intermediate lower answers are body witnesses and are not top-level closure rows. |
| Strict negation | Current head root; negative body calls read `evaluation_lower/3` from `LowerRows` | Preserved. The lower snapshot is unioned before the next stratum. |
| Count aggregate | Aggregate rule head from `CurrentRules`; aggregate output row is in `AggregateSeeds` and therefore in `StratumSeeds` | Preserved only when roots use `CurrentRules`, not `PlainRules`. Grouped east/west count probe produced 2 and 1. |
| Seed-only relation | Relation extracted from `StratumSeeds` | Preserved. A seed-only `ref(only)` row was collected. |
| No-rule/no-seed program | Explicit `ref(kernel(nil))` root | Preserved as `[call(ref(kernel(nil)), [const([])])]`. |
| Generated `Option(text)` bind | Current `:` or generated head root; all `evaluation_request/2` rows collected after root proofs | Preserved. The bound lower `Option/2` call can remain an intermediate witness; its `kernel(intern)` request is retained. |
| Functional-key diagnostics | Final `Closure` retains all prior `LowerRows` and all current output roots | Preserved under the closure law. Key validation continues to run over the final union. |
| Hosted predicates | Hosted/compiler/logical inputs enter the emitter evaluator as ground seed calls; their relations enter `StratumSeeds` | Preserved for the existing emitter path. A hosted relation supplied only as an unrepresented external callback would violate the evaluator's existing seed contract. |
| Nested evaluations | `EvaluationId`, `LowerRows`, and `evaluation_request/2` remain evaluation-local; lower-store lifetime remains owned by `evaluate/4` | Preserved. The collector must use its explicit `LowerRows` argument and current `EvaluationId`, never process-global rows. |

## Rows that can arise outside `Roots`

There are three categories.

1. `kernel(nil)` is a top-level evaluator row outside authored rule heads and
   seeds. The candidate handles it by adding the explicit root.
2. `kernel(intern)` requests are recorded by side effect in
   `evaluation_request/2`, including requests generated while a lower
   demand-cone rule is called in a more instantiated mode. They are outside
   relation-root enumeration and must be appended independently.
3. `kernel(cons)`, `kernel(edge_ref)`, and integer-comparison calls are body
   witnesses. Their constructive clauses require bound inputs, so an
   unbound top-level `proves/2` traversal does not materialize them as closure
   rows. A current rule whose head is such a relation, or a direct evaluator
   caller that seeds one, would need that relation in `Roots`; checked DL7
   programs use these as goals rather than as ordinary authored output heads.

Lower relations can produce additional answers while a current root is being
proved. Those answers are nested proof witnesses. A lower relation is either
already in `LowerRows`, or it is a lower demand-cone definition whose answer
is consumed by the current root. The answer does not escape the outer
`findall/3`; the associated intern request does escape through
`evaluation_request/2` and is retained by rule 2 above.

Therefore every top-level row omitted by the candidate must be one of the
explicitly handled `kernel(nil)` or request cases, or a nested witness that
the existing general collector did not expose as an outer closure row.

## Bounded probes

All SWI invocations used `timeout 20` and ran serially.

The isolated candidate snapshot passed the focused evaluator cases for:

```text
positive lower dependency       passed
strict negation                 passed
count over completed lower rows passed
transitive recursion            passed
lower facts plus recursion      passed
constructive cons               passed
integer comparison              passed
```

Direct collector probes produced:

```text
positive dependency:
  source, blocked, upper(application(option,[primitive(text)])),
  kernel(intern)(option,[primitive(text)],application(option,[primitive(text)])),
  kernel(nil)

count aggregate:
  sale east/one, sale east/two, sale west/three,
  region_count east/2, region_count west/1, kernel(nil)

no rules and no seeds:
  kernel(nil)

seed-only relation:
  only(x), kernel(nil)
```

The current checkout's full `5_curry` compile independently reports
`source_refreeze_limit_exhausted(16)` before closure comparison, so that
fixture cannot serve as an isolated closure-candidate oracle in this shared
checkout. The closure-root tests above do not depend on that compiler-round
failure.

## Review result

The closure-root optimization is semantically closed under the current
evaluator model after the arity/wiring requirement is satisfied. The required
landing checks are:

- `CurrentRules` is an explicit `evaluate_stratum_after_aggregates/14`
  argument at the call site and in both clauses.
- `current_result_relations/3` includes all current rule heads, including
  aggregate heads, all bound seed relations, and `ref(kernel(nil))`.
- `collect_closure/4` appends all current-evaluation requests and unions the
  sorted delta with the ordered `LowerRows` snapshot.
- Rule installation, demand-cone selection, proof table identity, cleanup,
  and lower-store ownership remain unchanged.
