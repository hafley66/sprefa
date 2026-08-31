---
created: 2026-08-31
updated: 2026-08-31
type: task
status: open
priority: high
epic: dl7-type-algebra
labels:
- size:large
size: L
lane: dl7-type-algebra
lane_seq: 2
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-conformance-apply']
---

# Derive structural conformance proofs

## Description

## Description

Compare frozen product edges, derive missing_contract_edge rows, and derive Conforms only when every contract edge matches.

## Acceptance Criteria

- [ ] Extra source edges are accepted.
- [ ] Missing or incompatible edges block the proof.
- [ ] Failure rows retain source, contract, label, target, and ordinal.

## Tests Run

- [ ] Positive and negative structural fixtures pass.
