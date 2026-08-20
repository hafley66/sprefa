---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: open
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
---

# Freeze canonical type rows after minting

## Description

Plan: `v6/plans/2026-08-20-canonical-type-row-pipeline.md`.

Move the semantic type-row freeze after generic, anonymous, option, enum, key,
and annotation minting. Establish one complete semantic authority before
compiler queries and physical lowering.

## Acceptance Criteria

- [ ] Every declared and generated type has one canonical declaration row.
- [ ] Every declared and generated field has one canonical member row.
- [ ] Wrapper and generic applications retain ordered semantic arguments.
- [ ] Duplicate semantic identities produce a named compiler refusal.
- [ ] Compiler and oracle produce equal canonical row sets.

## Tests Run

## Implementation Notes
