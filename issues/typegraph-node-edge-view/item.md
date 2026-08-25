---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: codex
status: in-progress
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 20
collision: [generic-type-core, catalog-schema]
blocked_by: ['@typegraph-integration-plan', '@canonical-type-reflection', '@dot-brace-nesting']
---

# Expose canonical type nodes and edges to user-land DL6

## Description

Expose a stable user-land view of canonical type nodes and relationships during the compiler fixpoint. Preserve specialized semantic rows and serialized contracts unless the approved plan replaces them.

## Provisional Signatures

```dl6
$type.node(TypeId, Kind, Name).
$type.edge(EdgeId, OwnerId, Role, Position, Name, TargetId).
```

Roles cover members, nesting, variants, constructors, arguments, derivation, and annotation sites.

## Lifetime and Uniqueness

Rows appear after canonical freeze, participate in refreeze rounds, and erase before runtime unless exported. Edge IDs are stable. Ordered edges use `(Owner, Role, Position)` uniqueness.

## Acceptance Criteria

- [ ] Authored compiler rules query node and edge rows.
- [ ] Members, arguments, applications, paths, variants, and annotation sites retain stable identity.
- [ ] Programs not querying the view keep byte-identical artifacts.
- [ ] Module-qualified and generated generic identities survive.
- [ ] Later refreeze rounds can consume these rows.

## Tests Run

Canonical reflection tests, complete compiler suite, typegen golden.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop communication and completion hail. Blocked by the approved plan and `@canonical-type-reflection`.

## Decisions

### 2026-08-24T23:14:05Z · @codex

Node, edge, path, projection, and annotation views range over authored and compiler-generated canonical rows. Undeclared rule-head IDBs remain outside these views. `@dot-brace-nesting` now blocks this card so canonical path receipts use the final name-prefix semantics.

## Agent Runs

### 2026-08-25T00:24:49Z · @codex

Direct implementation started from main c8940453a. First slice reuses preserved projection oracle 0bedd36a5 after adapting it to name-prefix-only nesting. Scope remains declared and compiler-generated canonical rows, with no undeclared-IDB inference. Subsequent slices expose type.node/type.edge compiler sources, add path and annotation-site identities, validate functional dependencies, and prove runtime erasure.
