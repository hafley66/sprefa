---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: terra
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:temporal
- size:med
- model:medium
size: M
lane: temporal
lane_seq: 20
collision: [generic-type-core, compiler-oracle]
blocked_by: ['@userland-integrity-graph', '@userland-flow-graph', '@userland-materialization-graph']
---

# Derive temporal semantics from user-land annotations

## Description

Replace temporal request builtins with ordinary call-form DL6 annotations that derive Flow Graph and Materialization Graph rows. Preserve temporal-v2 behavior before suffix removal.

## Surface Sketch

```dl6
temporal.log(Event).
temporal.keep(Event, all).
temporal.history(Entity).
```

## Lowering Sketch

```text
annotation application
  -> flow occurrence and retention facts
  -> storage requirements
  -> target plans
```

`history` must follow the selected whole-state, delta, causal-event, identity, sequence, and timestamp rulings.

## Lifetime

Annotation and derived semantic rows exist during compiler refreeze. Retained runtime records follow the derived policy. Reclamation emits a minus occurrence when the selected ruling requires it.

## Storage And Uniqueness

Event occurrence identity and retained-record identity are separate. History identity includes the entity key plus sequence/version; timestamps are data and ordering evidence according to the selected contract.

## Acceptance Criteria

- [ ] `log`, `keep`, and `history` are ordinary DL6 declarations and rules.
- [ ] Temporal request builtins are removed after parity.
- [ ] Flow and storage rows contain no target name.
- [ ] Invalid targets, policies, and conflicts retain diagnostics.
- [ ] Call syntax matches legacy runtime declarations and timelines.
- [ ] Compiler-only semantic rows create no runtime tables.

## Tests Run

Temporal compiler tests, runtime timelines, retention, history identity, target plan snapshots.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification.
