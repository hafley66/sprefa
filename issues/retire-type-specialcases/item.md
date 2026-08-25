---
created: 2026-08-24
updated: 2026-08-25
type: task
assignee: terra
status: open
priority: normal
epic: relational-semantic-planes
labels:
- area:dl6
- area:compiler
- intent:cleanup
- size:med
- model:medium
size: M
lane: semantic-cleanup
lane_seq: 10
collision: [generic-type-core, storage-lowering]
blocked_by: ['@member-edge-relational-view', '@relation-application-semantics', '@userland-integrity-graph', '@userland-flow-graph', '@userland-materialization-graph', '@userland-temporal-annotations']
---

# Retire superseded host compiler semantic special cases

## Description

Remove feature-specific host semantics after user-land replacements and target plans have parity. Every removal requires before and after reference counts.

## Candidate Removals

- Temporal request builtin handling.
- Key-wrapper collection superseded by Integrity Graph rules.
- Private clock discovery inputs superseded by Flow Graph projection.
- Plane-bearing storage interpretation superseded by Materialization Graph rules.
- Compatibility type carriers selected for retirement by the rulings card.
- Dead declarations, diagnostics, and fixtures.

## Acceptance Criteria

- [ ] Every candidate has before and after definition and call-site counts.
- [ ] Replacement relations have focused parity tests.
- [ ] Host PL retains only approved generic fixpoint, interning, diagnostics, and lowering mechanisms.
- [ ] Migrated fixtures keep runtime and generated artifact equality.
- [ ] No compatibility shim recreates removed feature semantics.

## Tests Run

Complete PLUnit compiler suite, fixture matrix, SQLite execution, Rust execution, generated artifact snapshots. TypeScript-v2 tests are excluded.

## Implementation Notes

Execution tier: Medium, size `M`, model `medium`. Native Terra-high with Boop completion notification. Deletion begins only after all replacement cards are complete.
