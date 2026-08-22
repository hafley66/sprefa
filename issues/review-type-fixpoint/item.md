---
created: 2026-08-21
updated: 2026-08-22
type: task
assignee: chris
status: done
priority: high
epic: comptime-type-model
labels:
- area:dl6
- intent:decision
related: ['@compiler-type-relations', '@semantic-type-identity']
closed: 2026-08-22
closed_by: terra
---

# Review type evaluation and specialization fixpoint

## Description

Review how type-returning relations and generic discovery iterate to a stable result. Produce a decision only.

Review evaluation order for `Box(first(int))`, compiler facts carrying
`Box(int)`, nested applications, recursion, stable identities, and termination.

## Review Alternatives

- Retain one-pass evaluation and reject type applications discovered later.
- Use a bounded evaluate/discover/specialize/freeze fixpoint.
- Permit generated types to trigger another query only within an explicit
  query-depth or frontier limit.

## Acceptance Criteria

- [x] Compare one-pass staging with a bounded evaluate/discover/specialize/freeze fixpoint.
- [x] Specify the frontier identity and termination condition.
- [x] Specify duplicate, oscillation, and limit diagnostics.
- [x] Decide whether generated types may trigger another type query.
- [x] Record the user-confirmed ruling or explicit deferral.

## Tests Run

Review card. No implementation CI.

## Implementation Notes

Do not reorder expansion phases from this card.

## Decisions

### 2026-08-22T21:21:07Z · @terra

Ruling (2026-08-22): adopt a bounded freeze/evaluate/discover/specialize/refreeze loop for closed named fixed-arity constructors. Frontier identity is application(ConstructorTypeId, OrderedArgumentTypeIds); requests deduplicate by that semantic identity. Each compiler round observes immutable canonical $type source rows. Newly minted declarations are frozen, then may trigger the next compiler-query round. Terminate when the canonical semantic type-row set is stable. Refuse constructor-producing recursive SCCs by name. Emit named diagnostics for arity mismatch, unknown constructor, non-ground applications after joins, recursive construction, and round-limit exhaustion where reachable. Preserve existing generic/wrapper/enum/anonymous minting authority, compiler-plane erasure, and the positive safe set-fixpoint evaluator.

## Resolution

### 2026-08-22T21:21:08Z · @terra

User-confirmed bounded closed-constructor refreeze ruling recorded in Decisions.
