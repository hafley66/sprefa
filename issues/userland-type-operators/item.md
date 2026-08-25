---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: terra
status: open
priority: normal
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:type-system
- size:med
- model:medium
size: M
lane: typegraph-core
lane_seq: 50
collision: [generic-type-core]
blocked_by: ['@typegraph-node-edge-view', '@typegraph-member-planes', '@type-pattern-lowering', '@compiler-plane-expression-parity', '@compiler-stratified-negation']
---

# Define impl concat inherit and extends as user-land relations

## Description

Prove common type operators can be ordinary DL6 libraries over canonical graph rows without compiler builtins.

## Required Demonstrators

`serializable(Type)`, `Partial(Type)`, `extends(Child, Parent)`, `impl(Type, Interface)`, and `concat(Left, Right, Output)`.

`Partial` derives optional logical members. `extends` and `impl` are graph relations. `serializable` recursively follows targets. `concat` derives generated members with collision diagnostics.

## Acceptance Criteria

- [ ] Every demonstrator is DL6 library code.
- [ ] `Partial(T)` materializes through bounded refreeze.
- [ ] Recursive serializability has a cycle policy.
- [ ] `extends` and `impl` support transitive queries.
- [ ] `concat` preserves order and rejects incompatible names.
- [ ] No higher-kinded feature is required.
- [ ] New operators using existing rows require only DL6 code.

## Tests Run

Compiler fixpoint, generated relation, recursive graph, and cross-target tests.

## Implementation Notes

Execution tier: Medium, size `M`, label `size:med`. Native Terra-high with Boop completion hail. Blocked by node/edge rows, member planes, and structural patterns.
