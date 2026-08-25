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
- intent:storage
- size:large
- model:large
size: L
lane: materialization
lane_seq: 10
collision: [storage-lowering, compiler-oracle, generic-type-core]
blocked_by: ['@userland-integrity-graph', '@userland-flow-graph', '@canonical-storage-projection']
---

# Derive target-neutral materialization requirements

## Description

Derive the logical storage needed to execute Flow Graph semantics. Keep target spelling and target capabilities in target plans.

## Provisional Signatures

```dl6
rel storage.relation(Relation: key(type), Purpose: key(type), Shape: type).
rel storage.column(Storage: key(type), Edge: type.edge, Target: type, Index: int).
rel storage.key(Storage: key(type), Group: key(type), Edge: type.edge, Index: int).
rel storage.retention(Storage: key(type), Policy: type).
```

## Lowering Sketch

```text
type.* + integrity.* + flow.*
  -> storage requirements
  -> target capability check
  -> target plan names and artifacts
```

## Lifetime

Materialization rows exist during compiler refreeze. Target plans create, migrate, or remove physical artifacts from the current accepted set.

## Storage And Uniqueness

Logical storage identity is `(Relation, Purpose)`. Target-specific names are a separate mapping. Companion artifacts use structural roles instead of string suffix inference.

## Acceptance Criteria

- [ ] B maps to current-state requirements.
- [ ] N maps to occurrence-trace requirements.
- [ ] Z maps to delta-frontier requirements.
- [ ] Pre-boundary, carry, and delayed SCC requirements are representable.
- [ ] Requirement versus hint behavior follows the selected ruling.
- [ ] No target name appears in a target-neutral storage relation.

## Tests Run

Storage projection snapshots, temporal timelines, stale artifact retraction, target plan comparisons.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Direct work because this defines the semantic boundary shared by emitters.
