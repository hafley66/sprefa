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

# Expose canonical type rows to compiler relations

## Description

Plan: `v6/plans/2026-08-20-type-surface-gaps.md`.

Expose canonical declarations, members, roles, applications, and arguments to
authored compiler relations, then erase those projections before runtime
planning.

Plan: `v6/plans/2026-08-20-canonical-type-row-pipeline.md`.

Let authored compiler relations iterate canonical declaration, member, and
application rows through derived, storage-free views. No parallel field
representation.

## Acceptance Criteria

- [ ] Authored compiler rules iterate one row per canonical member.
- [ ] Module-qualified, generic, wrapper, and anonymous field types resolve.
- [ ] Field order and exact casing survive the query boundary.
- [ ] Reflection views are absent from SQL, boot data, DD inputs, and hosts.
- [ ] Type artifact CI compiles generated TypeScript and Rust.

## Tests Run

## Implementation Notes
