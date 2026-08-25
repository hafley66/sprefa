---
created: 2026-08-25
updated: 2026-08-25
type: task
assignee: codex
status: open
priority: high
epic: relational-semantic-planes
labels:
- area:dl6
- intent:design
- size:large
- model:large
size: L
lane: semantic-planes
lane_seq: 10
collision: [compiler-plans, generic-type-core]
---

# Resolve semantic plane contracts

## Description

Record the contracts that change representation, identity, or target boundaries before implementation. Use `plans/2026-08-25-relational-semantic-planes.md` as the decision ledger.

## Rulings

- Node classification for product, sum, namespace, generic parameter, and generated anonymous relation nodes.
- Edge classification for field, variant, contains, parameter, return, and annotation roles.
- Edge-valued reference and annotation syntax for keyed `type.edge` rows.
- Canonical identity for anonymous product and sum relation nodes.
- Carrier retirement scope for internal `TypeId`, legacy `MemberId`, `application(...)`, `member_role`, and `rel/5` compatibility rows.
- Edge identity and migration from legacy `(Owner, Position, Name)` toward `(Owner, Name)`.
- Integrity identity versus flow replacement identity.
- User-land surfaces for unique, reference, and check annotations.
- State, event, history, and retention vocabulary after the prior `rel(0)`, `rel(1)`, `rel` ruling.
- Whole-state, delta, and causal-event history representation.
- Retention reclamation visibility as a Flow Graph minus occurrence.
- Clock/cardinality refusal versus explicit boundary policy.
- Per-row consumption semantics for CSP protocols.
- Materialization requirement versus hint semantics.
- Target artifact role enum and target-private extension.
- Emitter capability contract.

## Acceptance Criteria

- [ ] Every ruling has one selected contract and rejected alternatives recorded as data or comments.
- [ ] Each selected contract has a type signature.
- [ ] Each selected contract has an instance timeline.
- [ ] Each selected contract states storage, reads, writes, and uniqueness.
- [ ] No compiler implementation changes are included.
- [ ] Dependent cards cite the exact selected contracts.

## Tests Run

Documentation and issue DAG validation only.

## Implementation Notes

Execution tier: Large, size `L`, model `large`. Direct design work. Any choice absent from prior user rulings yields for user input.
