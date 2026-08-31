---
created: 2026-08-31
updated: 2026-08-31
type: epic
owner: chris
status: open
priority: high
labels: [compiler]
---

# DL7 userland type algebra

## Description

## Description

Implement plans/2026-08-31-dl7-type-algebra.md: structural contracts, ordinary conformance proofs, interface intersection, generic constraints, impl evidence, relation-valued edges, and HistoryV1 contract checks.

## Acceptance Criteria

- [ ] All child tasks close with commit receipts.
- [ ] Contracts and proofs are ordinary DL7 relations.
- [ ] Intersection emits deterministic product edges and conflict facts.
- [ ] Generic constraints and impl evidence use conformance goals.
- [ ] HistoryV1 requires an authored contract option.

## Tests Run

- [ ] Complete V7 SWI suite passes.
- [ ] Consolidated Tree-sitter corpus passes.

## Implementation Notes

Branch: feature/dl7-type-algebra. The expression-flow implementation is included in the branch ancestry.
