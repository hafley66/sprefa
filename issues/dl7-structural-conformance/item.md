---
created: 2026-08-31
updated: 2026-08-31
type: task
status: done
priority: high
epic: dl7-type-algebra
labels:
- size:large
size: L
lane: dl7-type-algebra
lane_seq: 2
collision: [v7-prelude, v7-test]
blocked_by: ['@dl7-conformance-apply']
commits:
- hash: '1095987e5'
  summary: Draft DL7 userland type algebra
- hash: c16546295
  summary: Prove DL7 userland type algebra
closed: 2026-08-31
---

# Derive structural conformance proofs

## Description

## Description

Compare frozen product edges, derive missing_contract_edge rows, and derive Conforms only when every contract edge matches.

## Acceptance Criteria

- [x] Extra source edges are accepted.
- [x] Missing or incompatible edges block the proof.
- [x] Failure rows retain source, contract, label, target, and ordinal.

## Tests Run

- [x] Positive and negative structural fixtures pass.
