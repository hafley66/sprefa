---
created: 2026-08-20
updated: 2026-08-24
type: bug
assignee: codex
status: fixed
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
closed: 2026-08-24
closed_by: codex
commits:
- hash: 2e9c3c792
  summary: 'dl6: correct typed annotation application semantics'
---

# Correct typed annotation application semantics

## Description

Reconcile the read-only Fable review of a5929de0a.

## Acceptance Criteria

- [x] Nested application sites retain their complete source path and deterministic ordering.
- [x] Annotation relation and application identities remain module-qualified.
- [x] Annotation key bridging precedes relation mirror construction.
- [x] Dead evaluator clauses are removed or covered by reachable behavior.
- [x] `-> ExistingColumn` aliases a declared type column as return without authored identity facts.
- [x] Compiler metadata erasure and userland emitter transport have executable CI.

## Tests Run

Pending.

## Implementation Notes

Evidence: Boop ACP session df47dba4-a6be-45bd-8df2-763461ede5e1, turn 35.

## Comments

### 2026-08-24T22:15:24Z · @codex

2026-08-24 verification on main `2c366a932`: commit `2e9c3c792` is present. `run_tests([compiler_relations,annotation_surface,type_relation_ir])` passed 101/101. `run_tests(braced_nested_relations)` passed 26/26, including annotation evidence path coverage. `bash compile/test/typegen_golden.sh` completed with `TYPEGEN GOLDEN: HOLDS`, including real DL6 TypeScript, Rust, and schema checks for `type-annotation-ci`. Existing singleton and choicepoint warnings remained.

## Resolution

### 2026-08-24T22:15:31Z · @codex

Acceptance criteria verified on current main; focused compiler/type tests and the cross-target typegen golden hold.
