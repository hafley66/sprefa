---
created: 2026-08-25
updated: 2026-08-25
type: task
assignee: codex
status: in-progress
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:semantics
- size:large
- model:large
size: L
---

# Stratified negation in compiler-plane DL6

## Description

Allow safe compiler relations to use the existing not(Relation(...)) body form against completed lower strata. This supplies closed-world complement over a finite frozen type graph and lets userland serializability handle recursive type SCCs without depth counters or recursive aggregates.

## Required Signatures

```dl6
serializable(Type) <-
  serialization_candidate(Type),
  not(serialization_blocked(Type)).
```

Compiler dependency edges carry a gap:

```text
positive body relation  gap 0
negated body relation   gap 1
aggregate input         gap 1
```

The evaluator receives each stratum as a completed row set. A negated goal is
an anti-join against rows completed below the current stratum.

## Timeline

1. Positive rules below a negation close under the existing tabled fixpoint.
2. Aggregate rows for the next stratum read the completed lower rows.
3. Negated body goals in the next stratum read the same completed lower rows.
4. Positive rules in that stratum close under tabling.
5. Type construction requests continue through the existing bounded refreeze.

## Storage and Uniqueness

Negation creates no row, table, target payload, or runtime rule. It filters
candidate substitutions against the sorted compiler row set. Existing relation
keys and set semantics continue to determine uniqueness.

## Acceptance Criteria

- [ ] Compiler relations accept `not(Relation(...))` after its variables are bound.
- [ ] Negated dependencies require a completed lower stratum.
- [ ] Positive recursion below a negated consumer reaches its complete fixpoint.
- [ ] Cycles containing a negated dependency receive a named diagnostic.
- [ ] Unbound variables inside negation receive a named safety diagnostic.
- [ ] Negation and grouped count share one deterministic stratum schedule.
- [ ] Compiler negation rows and rules erase before runtime planning.
- [ ] No type-operator-specific host builtin is added.

## Tests Run

Focused compiler evaluator, safety, recursive closure, aggregate interaction,
refreeze, erasure, and complete Prolog gates.

## Implementation Notes

Execution tier: Large. Current Codex owns the compiler-semantic work directly.
The first consumer is `@userland-type-operators`, whose serializability rule can
compute positive `serialization_blocked` reachability and then take a stratified
complement over the finite frozen type graph.
