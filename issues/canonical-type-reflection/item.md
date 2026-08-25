---
created: 2026-08-20
updated: 2026-08-24
type: feature
assignee: codex
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@canonical-type-freeze']
closed: 2026-08-24
closed_by: codex
commits:
- hash: bef4acbb3
  summary: 'test(dl6): cover canonical type reflection artifacts'
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

- [x] Authored compiler rules iterate one row per canonical member.
- [x] Module-qualified, generic, wrapper, and anonymous field types resolve.
- [x] Field order and exact casing survive the query boundary.
- [x] Reflection views are absent from SQL, boot data, DD inputs, and hosts.
- [x] Type artifact CI compiles generated TypeScript and Rust.

## Tests Run

## Implementation Notes

## Comments

### 2026-08-24T22:15:23Z · @codex

2026-08-24 verification on main `2c366a932`: commit `bef4acbb3` is present. `run_tests([compiler_relations,annotation_surface,type_relation_ir])` passed 101/101. `run_tests(braced_nested_relations)` passed 26/26. `bash compile/test/typegen_golden.sh` completed with `TYPEGEN GOLDEN: HOLDS`, including real DL6 TypeScript, Rust, and schema checks for `type-reflection`. Existing singleton and choicepoint warnings remained.

## Resolution

### 2026-08-24T22:15:31Z · @codex

Acceptance criteria verified on current main; focused compiler/type tests and the cross-target typegen golden hold.
