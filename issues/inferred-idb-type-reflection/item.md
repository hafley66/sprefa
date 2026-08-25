---
created: 2026-08-24
updated: 2026-08-24
type: feature
assignee: codex
status: open
priority: low
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:future
- size:large
- model:large
lane: typegraph-backlog
lane_seq: 0
related: ['@canonical-storage-projection']
collision: [generic-type-core, compiler-plans, storage-lowering]
size: L
---

# Reflect undeclared IDB schemas as types

## Description

## Description

Investigate optional canonical type reflection for a runtime IDB introduced only by a rule head. Active compiler semantics keep such relations runtime-only under `Name/Arity`; this card must preserve that default unless a later language ruling explicitly changes it.

## Required Analysis

1. Separate Datalog predicate identity from declared schema identity.
2. Define whether reflection is opt-in, inferred, or declaration sugar.
3. Define logical type stability independently from `Initial` and `Schedule` witnesses.
4. Define module/path ownership when several rules or modules contribute to one IDB.
5. Keep runtime unification and fixpoint behavior independent from type reflection.

## Acceptance Criteria

- [ ] Existing undeclared IDBs remain absent from compiler type rows by default.
- [ ] Any proposed reflection surface has explicit identity, provenance, and stability rules.
- [ ] Runtime `rel/5` planning does not require a canonical `TypeId` for undeclared IDBs.
- [ ] No inferred `col_type/3` carrier changes a column origin from inferred to declared.

## Tests Run

Future design card. No active implementation dependency.

## Implementation Notes

Execution tier: Large. Backlog only. This issue blocks no active user-land type-graph card.
