# Demand-cone index lifetime review

## Status

Read-only review of the lifetime refinement after `53778e948`.
The intended refinement hoists the demand-cone indexes from one construction
per stratum to one construction per `evaluate/4` call, then threads the index
through `evaluate_strata`. No source file, test file, commit, or push was made
by this lane. The only owned artifact is this receipt.

During the first read, the shared checkout was between source edits: it had
the hoisted call sites but no visible definitions for
`demand_cone_static_indexes/4` or `demand_cone_rules_indexed/4`. The focused
evaluator tests reported `Unknown procedure:
dl7_evaluator:demand_cone_static_indexes/4` at that transient point. The
final source now contains both definitions and the required signatures.

## Current and refined call paths

At `53778e948`, the selector itself is indexed, but each
`demand_cone_rules/6` call rebuilds both indexes:

```prolog
demand_cone_rules(
    +Strata, +Level, +Rules, +Dependencies, +CurrentRules,
    -PlainRules) is det.
```

The current evaluator path is [`0_evaluator.pl:136-148`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:136). The refinement's intended path is:

```text
evaluate/4
  -> rule_dependencies/2
  -> stratify_rules_with_dependencies/4
  -> demand_cone_static_indexes/4       once for this evaluate/4
  -> evaluate_strata/10                 same StaticIndexes each level
       -> demand_cone_rules_indexed/4
       -> evaluate_strata/10 recursively
```

Required signatures:

```prolog
demand_cone_static_indexes(
    +Strata, +Rules, +Dependencies, -StaticIndexes) is det.

demand_cone_rules_indexed(
    +Level, +StaticIndexes, +CurrentRules, -PlainRules) is det.

evaluate_strata(
    +Level, +MaxStratum, +Strata, +Dependencies, +StaticIndexes,
    +Rules, +Seeds, +LowerRows, -Closure, -Diagnostics) is det.

evaluate_stratum_after_aggregates(
    +AggregateDiagnostics, +AggregateSeeds,
    +Level, +MaxStratum, +Strata, +Dependencies, +StaticIndexes,
    +Rules, +Seeds, +LowerRows,
    +PlainRules, +CurrentRules, +CurrentSeeds,
    -Closure, -Diagnostics) is det.
```

The current direct-test compatibility wrapper should remain:

```prolog
demand_cone_rules(
    +Strata, +Level, +Rules, +Dependencies, +CurrentRules,
    -PlainRules) is det.
```

That wrapper can build temporary indexes for qualified tests and delegate to
`demand_cone_rules_indexed/4`. The production evaluator must call the indexed
form with the static term passed from `evaluate_after_stratify/7`.

## Static index contents

`StaticIndexes` should be a ground compound carrying two assoc values:

```text
DependencyIndex:
  exact HeadRelation -> sorted unique BodyRelations
  only dependency(Head,Body,positive,0,positive)

RuleIndex:
  exact Relation -> RuleLevel-Rule entries
  only declared relations and nonaggregate rules
```

`DependencyIndex` has no `Level` field because the exact positive gap-0
relation graph is stable for the whole evaluate call. `RuleIndex` retains
`RuleLevel` because each stratum query needs the original `RuleLevel =< Level`
filter. A level-filtered map built once per stratum would preserve semantics,
but it would leave the index construction repeated across strata.

The indexed selector must perform:

```text
Roots = sort(CurrentRules excluding aggregate rules)
RootRelations = sorted unique heads(Roots)
SeenRelations = RootRelations
IncludedRelations = empty
Selected = Roots

for each discovered relation:
  read DependencyIndex[Relation], or []
  discover each unseen body relation once
  read RuleIndex[Relation], or []
  add entries whose RuleLevel =< Level once per relation

PlainRules = sort(Selected)
```

`SeenRelations` and `IncludedRelations` remain separate. A root relation is
already seen for queue purposes, while its current plain definitions are
already present in `Selected`. A relation-level map that combines these two
states can drop a same-head rule when the root relation later appears as a
body relation.

## Lifetime and cleanup invariants

1. `StaticIndexes` is a lexical argument owned by one `evaluate/4` call. No
   dynamic predicate, process-global cache, or compiler memo table stores it.
2. Build the index inside the existing `setup_call_cleanup/3` body that owns
   `open_lower_store/0` and `close_lower_store/0`. If index construction,
   stratum evaluation, or cleanup raises, the lower store still unwinds.
3. A nested `evaluate/4` constructs a separate term and passes it through its
   own recursive loop. The outer index remains on the outer Prolog stack and
   is never consulted by the nested call.
4. Each compiler round invokes a separate `evaluate/4` with its own `Rules`,
   `Dependencies`, and `Strata`, so generated rules cannot reuse an earlier
   index.
5. Empty `Rules` and `Dependencies` produce empty assoc values. Empty
   `Strata` still runs the existing level-0 evaluator path, preserving the
   `kernel(nil)` closure row and seed-only behavior from the closure collector.
6. A stratification diagnostic skips index construction through the existing
   `evaluate_after_stratify/7` diagnostic clause and returns the diagnostic
   with an empty closure.
7. Every recursive `evaluate_strata/10` clause, base clause, and
   `evaluate_stratum_after_aggregates/14` clause has the same StaticIndexes
   position and arity.

The transient mid-edit checkout violated item 7 at the implementation
boundary. The final source has the static-index definitions at
[`0_evaluator.pl:201`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:201),
the `evaluate_strata/10` threading at
[`0_evaluator.pl:135`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:135),
and the `evaluate_stratum_after_aggregates/14` threading at
[`0_evaluator.pl:154`](/Users/chrishafley/projects/sprefa/v7/src/1_libtime/0_evaluator.pl:154).

## Eligibility and exact-set invariants

The static `RuleIndex` must preserve the current per-stratum predicate:

```prolog
Rule = rule(call(Relation, _), _),
memberchk(stratum(Relation, RuleLevel), Strata),
RuleLevel =< Level,
\+ aggregate_rule(Rule).
```

Consequences:

- Relations absent from `Strata` have no indexed definitions.
- Future-level definitions remain excluded at lower `Level` values.
- Lower-level and same-level definitions remain eligible.
- Aggregate rules remain excluded even when they share a head relation with a
  plain rule.
- Every same-head plain definition is retained in the value list.
- Exact duplicate rule terms may occur in index values, then disappear at the
  existing final `sort/2`, matching the old selector.
- Dependency index entries retain only positive gap-0 edges with cause
  `positive`; negative and aggregate edges remain lower-snapshot reads.

## Bounded exact-set comparisons

The comparison script built one static dependency/rule index from the complete
checked program, then ran the existing per-stratum selector and a static-index
selector for each level. It compared complete sorted `PlainRules` lists with
`==`, while reporting counts.

```text
fixture: v7/test/fixtures/lexical_binding/7_nearest_shadow.dl7
level  old/indexed
0      29/29
1      23/23
2      53/53
3      21/21
4      17/17
5      75/75
6      2/2
all seven complete selected lists equal

fixture: v7/test/fixtures/2_partial.dl7
level  old/indexed
0      32/32
1      23/23
2      54/54
3      21/21
4      17/17
5      78/78
6      2/2
all seven complete selected lists equal
```

The synthetic reference suite in the committed per-stratum index change
covers same-head definitions, exact duplicate terms, positive cycles,
diamonds, lower-level definitions, future-level definitions, aggregate and
negative edge exclusion, and isolated current rules. It passed the direct
reference parity test. The direct wrapper tests for the transitive cone and
aggregate-edge exclusion passed `2/2` in one bounded SWI process.

## Required counterexamples

The static lifetime implementation must retain these cases:

| Case | Required result |
| --- | --- |
| Same head, `a :- b` and `a :- c` | Both `a` rules and both lower definitions remain selected. A map storing one rule per head loses one edge. |
| Positive cycle `a :- b`, `b :- a` | Each relation is expanded once; recursion terminates. |
| Diamond `d :- l,r`, both paths to `leaf` | `leaf` enters the queue once and its definitions are included once. |
| Lower dependency | A `RuleLevel=0` body definition remains available at `Level=1`. |
| Aggregate edge | `dependency(...,positive,1,aggregate)` contributes no demand-cone body relation. |
| Negative edge | `dependency(...,negative,1,negative)` contributes no demand-cone body relation. |
| Current rule with no dependency | The current plain root survives an empty dependency lookup. |
| Generated `Option(text)` | Exact relation identities and lower eligible rule definitions remain indexed; current stratum demand still reaches the lower constructor path. |
| Seed-only/no-rule | Static indexes are empty and selector output is `[]`; seeds are handled by evaluator seeds, not demand-cone rules. |
| Duplicate rule terms | Final selected terms match old `sort/2` output with one copy. |
| Nested evaluation | Inner and outer indexes remain separate lexical terms with no cache collision. |
| Exception during index construction | `close_lower_store/0` still runs through the outer `setup_call_cleanup/3`. |
| All strata | The same static maps are reused while `Level` changes; rule-level filtering occurs at each indexed selection. |

## Repeated construction quantified

Receipt 27 measured `15` production selector calls and `431,151` inclusive
selector inferences on nearest-shadow. The current per-stratum indexed path
constructs both `DependencyIndex` and `RuleIndex` inside each of those `15`
calls, so it performs `15` dependency-index constructions and `15`
rule-index constructions for that compile.

Receipt 24 attributes the three evaluate rounds to macrotime, compiler round
1, and compiler round 2. A correctly hoisted lifetime therefore constructs
the maps `3` times for that compile, once per distinct `evaluate/4` call, and
reuses each pair across that call's strata. The wrapper used by direct tests
may still construct temporary maps per invocation; that cost stays outside
the production evaluator path.

No global cache is required or permitted for this refinement. A cache keyed
by only `Rules` would be insufficient because `Strata` and `Dependencies` are
part of the eligibility/index contract, and compiler rounds can supply new
generated rules.

## Review result

Hoisting the two maps into one `evaluate/4` lexical term is selection-safe and
removes the repeated per-stratum index construction. The final focused subset
passed `12/12` in one SWI process under a 20-second timeout. It covered
evaluator trace, direct cone selectors, synthetic graph parity, both
fixture-level exact-set comparisons, closure/lower-row union, positive
dependency, strict negation, count aggregate, recursion, and mixed lower
facts with recursion. The earlier undefined-predicate failure was a transient
mid-edit observation and is not the final source result. No CI coverage was
added by this read-only review.
