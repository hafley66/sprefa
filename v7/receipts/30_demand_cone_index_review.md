# Demand-cone index review

## Status

Read-only design review of the proposed indexed demand-cone expansion on
current `main` at `3405509c6`. No source file, test file, commit, or push was
made by this lane. The only owned artifact is this receipt.

The current measured nearest-shadow contribution is `431,151` inclusive
inferences over `15` calls to `demand_cone_rules/6`; the source path is
[`0_evaluator.pl`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:184).

## Current call path and modes

The evaluator computes `CurrentRules` by exact stratum membership, then calls
the selector:

```prolog
evaluate_strata(
    +Level, +MaxStratum, +Strata, +Dependencies, +Rules, +Seeds,
    +LowerRows, -Closure, -Diagnostics)
```

at [`0_evaluator.pl:136-148`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:136). The current selector contract is:

```prolog
demand_cone_rules(
    +Strata, +Level, +Rules, +Dependencies, +CurrentRules,
    -PlainRules) is det.
```

`CurrentRules`, `Rules`, `Dependencies`, and `Strata` are ground. `Level` is
an integer. `CurrentRules` is the exact subset of `Rules` whose head relation
has `stratum(Relation, Level)`. The current implementation first removes
aggregate-headed roots, sorts them, then repeatedly performs:

```text
selected rules
  -> scan all Dependencies for positive gap-0 edges
  -> sort body relations
  -> scan all Rules for eligible definitions
  -> append and sort selected rules
  -> repeat until the sorted rule set is unchanged
```

The fixpoint is [`demand_cone_fixpoint/6`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:197). Its rule eligibility is exactly:

```prolog
Rule = rule(call(Relation, _), _),
memberchk(Relation, BodyRelations),
memberchk(stratum(Relation, RuleLevel), Strata),
RuleLevel =< Level,
\+ aggregate_rule(Rule).
```

The selector has two direct test call sites at [`1_entrypoints.test.pl:1941-1944`](/Users/chrishafley/projects/sprefa/v7/test/1_entrypoints.test.pl:1941) and [`1_entrypoints.test.pl:1963-1966`](/Users/chrishafley/projects/sprefa/v7/test/1_entrypoints.test.pl:1963). It is private from the module export surface, so evaluator integration and these qualified tests own its call contract.

## Proposed indexed shape

For the full evaluator cost target, build the static index once after
`rule_dependencies/2` and carry it through the stratum loop. A per-stratum
index removes repeated fixpoint joins but rescans `Rules` and `Dependencies`
for every level.

Recommended signatures:

```prolog
demand_cone_index(
    +Strata, +Rules, +Dependencies, -Index) is det.

demand_cone_rules_indexed(
    +Level, +Index, +CurrentRules, -PlainRules) is det.

demand_cone_worklist(
    +Queue0, +Seen0, +DependencyIndex, +RuleIndex,
    +Selected0, -Selected) is det.
```

The `Index` should contain:

```text
DependencyIndex:
  exact HeadRelation -> sorted unique BodyRelations
  source rows restricted to dependency(Head,Body,positive,0,positive)

RuleIndex:
  exact Relation -> plain rule definitions with their RuleLevel
  source rows restricted to declared Relation and nonaggregate Rule
```

The level filter can be applied while building a per-level `RuleIndex`, or the
static index can retain `RuleLevel` beside each rule and filter values with
`RuleLevel =< Level` when a relation is discovered. The static form avoids
rebuilding the same maps for all strata.

For compatibility with the existing qualified tests, a wrapper can retain the
current signature:

```prolog
demand_cone_rules(
    +Strata, +Level, +Rules, +Dependencies, +CurrentRules,
    -PlainRules) is det.
```

The evaluator path should use the indexed form. If the wrapper builds an index
on every call, selected-rule semantics remain correct, while the evaluation
still pays a per-stratum index construction cost.

The worklist algorithm is:

```text
Roots = sort(CurrentRules excluding aggregate rules)
Queue = sorted unique head relations of Roots
Selected = Roots
Seen = []

while Queue is nonempty:
  pop Relation
  if Relation ∈ Seen: continue
  add Relation to Seen
  BodyRelations = DependencyIndex[Relation], or []
  append each unseen BodyRelation to Queue
  append RuleIndex[Relation] values whose RuleLevel =< Level to Selected

PlainRules = sort(Selected)
```

The final `sort/2` is required. It preserves the existing installation order,
duplicate-term elimination, and exact `PlainRules` term set regardless of
worklist order.

## Equivalence invariants

For each relation `R`, the old fixpoint adds every eligible plain definition
with head `R` exactly when some selected rule has a positive gap-0 dependency
from its head to `R`. The indexed worklist must satisfy the same closure law:

```text
R is discovered
  iff R is a head relation of a root, or
     R is the BodyRelation of a positive gap-0 dependency
     whose HeadRelation was discovered.
```

The following conditions are required:

1. `DependencyIndex` uses the exact five-field dependency filter
   `positive, 0, positive`. Aggregate and negative edges are excluded.
2. `RuleIndex` excludes aggregate rules and relations absent from `Strata`.
3. `RuleIndex` admits every definition with `RuleLevel =< Level`, including
   all same-head definitions and all lower-level definitions.
4. The worklist has a relation-level `Seen` set. Positive recursion and
   cycles terminate after one relation expansion.
5. Every relation is enqueued at most once as a newly discovered relation.
   Duplicate dependency rows and diamond paths therefore cause no repeated
   index expansion.
6. Roots retain every current plain rule, including multiple definitions for
   one head relation. Root relation deduplication must not deduplicate rules.
7. Final `sort/2` removes exact duplicate rule terms, matching the old
   `sort(Next0, Next)` at every round.
8. Selector output remains a sorted list of complete rule terms. No row,
   proof, lower snapshot, aggregate, or evaluation-table behavior changes.

## Counterexample matrix

Each case below is a bounded selector case. The `old` and `indexed` outputs
were compared as complete sorted rule lists, not only as counts.

| Case | Synthetic shape | Regression caught | Expected selected rules |
| --- | --- | --- | --- |
| Same-head definitions | `a :- b`; `a :- c`; facts `b`, `c` | A head map retaining one definition loses one body edge or one root rule | Both `a` rules, `b`, `c` |
| Positive recursion | `a :- b`; `b :- a` | Queue without `Seen` loops or repeats work | `a`, `b` rules once each |
| Diamond | `d :- l,r`; `l :- leaf`; `r :- leaf`; fact `leaf` | Queue without relation dedup expands `leaf` twice | `d`, `l`, `r`, `leaf` |
| Lower-level dependency | `upper :- lower`, `stratum(lower,0)`, `stratum(upper,1)` | Rule index that only retains same-level definitions drops `lower` | `upper`, `lower` |
| Future-level guard | `current :- future`, `stratum(current,1)`, `stratum(future,2)` | Missing `RuleLevel =< Level` installs a future rule | `current` only |
| Aggregate edge | plain `shared :- needed`; aggregate `shared(count) :- aggregate_only` | Following all positive edges installs aggregate-only definition | `shared` plain rule and `needed` |
| Negative edge | `current :- not blocked`; fact `blocked` | Following negative edges moves lower-snapshot input into the cone | `current` only |
| No dependency | current fact rule | Root rule must survive an empty dependency index | current fact rule |
| Generated `Option(text)` | current generated edge rule calls lower `Option` relation | Exact `ref(...)` relation keys and lower rule eligibility must remain intact | current rule plus eligible `Option` definition |
| Seed-only/no-rule | no rules, seeds outside selector input | Selector must return `[]`; seeds enter evaluator closure separately | `[]` |
| Duplicate rule terms | the same rule appears twice in `Rules` and `CurrentRules` | Index values may duplicate, while final selected term set must match old sort | one copy |

The bounded synthetic families, including the future-level guard and duplicate
rule terms, returned exact equality between the current selector and the
indexed worklist. The generated `Option(text)` case used the same
positive lower relation shape as the nearest-shadow demand path.

## Relevant fixture comparisons

The comparison script loaded each checked fixture, derived its exact
`Dependencies` and `Strata`, selected `CurrentRules` per level, then ran both
selectors. Each level's complete sorted rule list was compared with `==`.

```text
fixture: v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
levels: 7
old/indexed rule counts:
  level 0: 29/29
  level 1: 23/23
  level 2: 53/53
  level 3: 21/21
  level 4: 17/17
  level 5: 75/75
  level 6: 2/2
complete sorted-list equality: true for all 7 levels

fixture: v7/test/fixtures/2_partial.dl7
levels: 7
old/indexed rule counts:
  level 0: 32/32
  level 1: 23/23
  level 2: 54/54
  level 3: 21/21
  level 4: 17/17
  level 5: 78/78
  level 6: 2/2
complete sorted-list equality: true for all 7 levels
```

The existing selector tests for the transitive cone and aggregate-edge
exclusion passed `2/2` in one bounded SWI process. The evaluator integration
case for a completed lower positive dependency also passed in the focused
subset. Each SWI invocation used `timeout 20`; processes were run serially.

## Review result

The indexed worklist is selection-equivalent to the current fixpoint when the
index filters and final sort above are retained. The implementation boundary
should carry one static index through `evaluate/4` if the measured
431,151-inference selector cost is the target. A wrapper preserving
`demand_cone_rules/6` can keep the existing direct test mode while delegating
to `demand_cone_rules_indexed/4`.

No kernel, graph, type, binding, aggregate, negation, lower-snapshot, or
macrotime semantics are implicated by this selector-only change. CI coverage
would add exact-list selector cases for same-head definitions, cycles,
diamonds, level filtering, duplicate rule terms, and the generated
`Option(text)` path; no coverage was added in this read-only review.
