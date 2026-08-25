---
created: 2026-08-25
updated: 2026-08-25
type: feature
assignee: codex
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:flow
- size:large
- model:large
size: L
lane: flow-semantics
lane_seq: 10
collision: [compiler-oracle, generic-type-core]
blocked_by: ['@semantic-plane-rulings', '@relation-application-semantics']
---

# Expose target-neutral flow semantics

## Description

Represent relation occurrences, signs, timing, replacement, and retention as ordinary DL6 facts consumed by the clock checker and materialization rules.

## Provisional Signatures

```dl6
rel flow.occurrence(Relation: key(type), Sign: sign, Clock: clock, Grade: grade).
rel flow.dependency(Reader: key(type), Writer: key(type), Sign: sign,
                    Delay: delay, Grade: grade, Role: role).
rel flow.identity(Relation: key(type), Group: key(type), Edge: type.edge, Index: int).
rel flow.retention(Relation: key(type), Policy: type).
```

Final names and field domains are selected by `@semantic-plane-rulings`.

## Lifetime

Flow rows are derived per compiled program and refreeze. Runtime occurrence data follows the selected clock and sign; retained records live according to retention policy.

## Storage And Uniqueness

Flow identity identifies replacement within a relation. It can reuse Integrity Graph evidence while remaining a separate semantic relation. Occurrence rows and retained records have separate keys.

## Acceptance Criteria

- [ ] B, N, and Z clock semantics have target-neutral rows.
- [ ] Positive, negative, pre-boundary, carry, and delayed dependencies are representable.
- [ ] Occurrence and retained-record identities are distinct.
- [ ] Current `clock_dependency/8` evidence projects into the new rows without loss.
- [ ] No SQLite, Rust, or TypeScript target terms appear in Flow Graph relations.

## Tests Run

Clock fact snapshots, dependency projection tests, temporal timelines, existing clock checker suite.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Direct work because clock and replacement semantics cross compiler and runtime boundaries.
