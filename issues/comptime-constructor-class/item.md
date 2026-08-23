---
created: 2026-08-22
updated: 2026-08-22
type: task
status: open
priority: normal
epic: comptime-type-model
related: ['@review-type-fixpoint', '@compiler-type-relations', '@semantic-type-identity', '@review-higher-kinds', '@type-relation-ir', '@type-annotation-eval']
labels:
- area:dl6
- intent:study
source_ref: chat_log/20260821.0.dl6-comptime-type-relational-macros.md
---

# Compiler class: Datalog type construction and the type space

## Description

Study how the already-landed DL6 compiler pieces compose when a compiler relation needs a type application that is absent from the current canonical graph.

This card is an index and teaching pass over prior work. It introduces no parallel type identity, evaluator, annotation system, or implementation authorization. The unresolved decision remains owned by `@review-type-fixpoint`.

## Prior Work

| Existing work | Established contract |
|---|---|
| `@semantic-type-identity` | An application TypeId is `application(ConstructorSemanticId, OrderedArgumentSemanticIds)`. Generated names remain artifact-boundary data. |
| `@compiler-type-relations` | Positive safe compiler rules use deterministic set fixpoint evaluation, functional-key conflict checks, and compiler-plane erasure. |
| `@type-relation-ir` | Canonical declarations, members, roles, applications, and arguments have target-independent semantic rows. |
| `@type-annotation-eval` | Direct type applications execute through compiler relations, return exactly one type, retain site evidence, and erase transport before runtime. |
| `chat_log/20260821.0.dl6-comptime-type-relational-macros.md` | Records the unified comptime/generic model, the `Box(first(int))` phase-order gap, and bounded query/refreeze as the existing direction. |
| `v6/plans/2026-08-20-canonical-type-row-pipeline.md` | Already specifies freeze, compiler queries, requested type construction, bounded refreeze, and physical lowering. |
| `@review-type-fixpoint` | Owns the decision about generated types triggering further queries, frontier identity, limits, and non-convergence. |
| `@review-higher-kinds` | Owns constructor-valued variables and kind signatures. A literal constructor such as `list` does not settle that ruling. |

## Narrow Question

Give relation-shaped semantics to a closed constructor application inside a compiler rule while preserving function-free Datalog lowering:

```dl6
output(T, ListT) <-
  input(T),
  type_apply(list, [T], ListT).
```

`type_apply` is a provisional compiler-IR name, not an authored spelling ruling.

## Type Apply Semantics

Signature:

```text
type_apply(
  ConstructorTypeId,
  OrderedArgumentTypeIds,
  ApplicationTypeId
)
```

Functional key:

```text
ConstructorTypeId + OrderedArgumentTypeIds -> ApplicationTypeId
```

Identity reuses the landed semantic representation:

```text
ApplicationTypeId =
  application(ConstructorTypeId, OrderedArgumentTypeIds)
```

No second registry or generated identity is introduced.

For one immutable compiler round:

1. Compute the structural ApplicationTypeId deterministically.
2. Bind the result immediately, allowing other compiler rows to refer to that identity.
3. If the canonical graph already contains the application, read its existing `type_application` and `type_argument` rows.
4. If absent, add the identity to the next construction frontier.
5. Keep the current `$type` snapshot unchanged for the rest of the round.

Between rounds:

1. Deduplicate the construction frontier by ApplicationTypeId.
2. Reuse existing generic and wrapper minting to materialize requested declarations and members.
3. Freeze a new canonical graph.
4. The next `$type` snapshot exposes the application, arguments, generated declaration, and members.
5. Repeat only under the policy selected by `@review-type-fixpoint`.

This separates two observations:

```text
The TypeId can exist structurally in round N.
Its member graph becomes queryable in round N + 1.
```

## Effect on the Type Space

`$type` is a frozen snapshot during each Datalog closure. `type_apply` may append a request to the next frontier; it does not mutate reflection rows while joins are running.

Existing application:

```text
$type round N
  -> application and argument rows found
  -> no frontier growth
  -> existing TypeId reused
```

Absent application:

```text
$type round N
  -> deterministic TypeId computed
  -> construction request deduplicated
  -> existing generic/wrapper minting runs
  -> freeze
$type round N + 1
  -> canonical rows now contain the application
```

Positive compiler rules make this growth monotone. Recursive constructor rules can still create an infinite sequence such as `T`, `list(T)`, `list(list(T))`; their allowance and diagnostic belong to `@review-type-fixpoint`.

## Class Outline

1. Function-free Datalog versus range restriction.
2. Existing-type lookup through `type_application/2` and `type_argument/4`.
3. Interpreted functional relations in a Datalog lowering.
4. Structural TypeId availability versus member-graph availability.
5. Frozen-round semantics and the next construction frontier.
6. Recursive construction and chase termination.
7. Rust monomorphization and trait solving versus TypeScript type instantiation.

## Acceptance Criteria

- [ ] Reproduce the established contracts from every Prior Work row without inventing replacements.
- [ ] Explain function-free Datalog separately from range restriction.
- [ ] Trace `type_apply` for an existing application and an absent application.
- [ ] Show that application identity reuses `@semantic-type-identity`.
- [ ] Draw the immutable `$type` round and next-frontier boundary.
- [ ] Separate literal constructors from the constructor-variable ruling in `@review-higher-kinds`.
- [ ] Connect termination alternatives only to `@review-type-fixpoint`.

## Tests Run

Study card. No implementation CI.

## Implementation Notes

No parser, evaluator, canonical-row, or generic-minting changes are authorized by this card.
