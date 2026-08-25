---
created: 2026-08-25
updated: 2026-08-25
type: task
assignee: terra
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:clock
- size:med
- model:medium
size: M
lane: flow-semantics
lane_seq: 20
collision: [compiler-oracle]
blocked_by: ['@userland-flow-graph']
---

# Feed the clock checker from Flow Graph facts

## Description

# Feed the clock checker from Flow Graph facts

## Description

Replace private discovery inputs to the clock checker with a lossless projection from `flow.*` relations. Preserve existing B, N, Z, sign, grade, role, SCC, and boundary behavior.

## Input Projection

```text
flow.dependency
flow.occurrence
flow.identity
flow.retention
  -> clock_dependency/8 compatibility view
  -> inferred_clock/4
  -> clock_scc/3
  -> clock_boundary/2
  -> clock_violation/2
```

## Acceptance Criteria

- [ ] Existing clock facts are reproducible from Flow Graph rows.
- [ ] The checker has no target-emitter dependency.
- [ ] The disabled path/cycle walk remains explicitly configured and tested.
- [ ] Existing violations and diagnostics remain byte-stable unless a ruling changes them.
- [ ] Compatibility inputs have a counted retirement path.

## Tests Run

Focused clock checker PLUnit, flow projection snapshots, SCC and boundary diagnostics.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification.
