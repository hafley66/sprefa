---
created: 2026-08-20
updated: 2026-08-20
type: bug
assignee: codex
status: open
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
---

# Correct typed annotation application semantics

## Description

Reconcile the read-only Fable review of a5929de0a.

## Acceptance Criteria

- [ ] Nested application sites retain their complete source path and deterministic ordering.
- [ ] Annotation relation and application identities remain module-qualified.
- [ ] Annotation key bridging precedes relation mirror construction.
- [ ] Dead evaluator clauses are removed or covered by reachable behavior.
- [ ] `-> ExistingColumn` aliases a declared type column as return without authored identity facts.
- [ ] Compiler metadata erasure and userland emitter transport have executable CI.

## Tests Run

Pending.

## Implementation Notes

Evidence: Boop ACP session df47dba4-a6be-45bd-8df2-763461ede5e1, turn 35.
