---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: in-progress
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@canonical-type-freeze']
---

# Derive physical storage from canonical type rows

## Description

Plan: `v6/plans/2026-08-20-canonical-type-row-pipeline.md`.

Replace post-freeze semantic reads of declaration carriers with
TypeId/MemberId-keyed storage projections, then serialize the catalog from
semantic and physical rows.

## Acceptance Criteria

- [ ] Storage relations reference canonical TypeId values.
- [ ] Storage columns reference canonical MemberId values.
- [ ] Physical rows contain target facts without copied semantic fields.
- [ ] Catalog rows are serialized from semantic and physical rows.
- [ ] TS, Rust, and SQLite executable CI passes.

## Tests Run

## Implementation Notes
