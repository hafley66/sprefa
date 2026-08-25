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
- intent:proof
- size:large
- model:large
size: L
lane: flow-semantics
lane_seq: 30
collision: [compiler-oracle, generic-type-core]
blocked_by: ['@clock-flow-projection', '@userland-integrity-graph']
---

# Prove flow cardinality and cycle safety

## Description

# Prove flow cardinality and cycle safety

## Description

Add the proof relations needed to answer whether a dependency cycle can produce a next value, whether a combine is deterministic, and whether a protocol has zero, one, or many producers or consumers.

## Provisional Signatures

```dl6
rel flow.cardinality(Relation: key(type), Boundary: clock, Minimum: int, Maximum: type).
rel flow.determinism(Relation: key(type), Mode: type).
rel flow.cycle(Scc: key(type), Member: type, Delay: delay).
rel flow.refusal(Subject: key(type), Reason: type).
```

## Lifetime

Proof rows exist during compiler refreeze. Runtime execution receives only accepted plans and any retained boundary metadata.

## Storage And Uniqueness

One selected proof result exists per subject and boundary. Conflicting derivations produce diagnostics rather than arbitrary selection.

## Acceptance Criteria

- [ ] Immediate cycles without a proven boundary or finite domain are detected.
- [ ] Delayed cycles distinguish available-next-tick values from unavailable values.
- [ ] Deterministic and nondeterministic combines are represented.
- [ ] Zero, one, and many producer/consumer cardinalities are represented.
- [ ] Refusal versus boundary policy follows the selected ruling.
- [ ] Existing clock SCC tests remain valid.

## Tests Run

Cycle, delayed-cycle, combine, producer-cardinality, consumer-cardinality, and diagnostics snapshots.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Direct proof-model work.
