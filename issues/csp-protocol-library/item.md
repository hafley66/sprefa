---
created: 2026-08-25
updated: 2026-08-25
type: feature
assignee: terra
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- intent:csp
- size:med
- model:medium
size: M
lane: protocols
lane_seq: 10
collision: [compiler-oracle, storage-lowering]
blocked_by: ['@flow-cardinality-proof', '@userland-materialization-graph']
---

# Express CSP protocols as DL6 libraries

## Description

# Express CSP protocols as DL6 libraries

## Description

Define bounded-channel and synchronization protocols as ordinary DL6 rules over Flow Graph, cardinality proofs, and materialization requirements.

## Protocol Inputs

```text
occurrence sign and clock
producer and consumer cardinality
replacement identity
retention policy
boundary and cycle proof
```

## Scope

- Exactly-once consumption.
- Capacity bounds.
- Empty/count behavior.
- Derived-trigger parity.
- Deterministic versus nondeterministic combine behavior.

## Lifetime

Protocol declarations are compile-time rows. Accepted plans may allocate retained arrangements or boundary state. Runtime occurrence rows advance according to the selected flow contract.

## Storage And Uniqueness

Protocol state keys use channel identity plus selected per-row consumption identity. The selected W1/W2 contract determines whether acknowledgement state is per occurrence, per consumer, or aggregate.

## Acceptance Criteria

- [ ] The existing 8-of-9 CSP lab cases are represented through plane facts.
- [ ] W1 exactly-once and W2 capacity have selected semantics and tests.
- [ ] W3 empty/count and W4 derived-trigger parity have tests.
- [ ] Protocol rules contain no target-emitter vocabulary.
- [ ] Required retained state is derived through the Materialization Graph.

## Tests Run

CSP lab matrix, cardinality proof snapshots, runtime timeline parity.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification.
