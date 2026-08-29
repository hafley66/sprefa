---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, basement, model-glm53f]
size: S
lane: dl7-basement
lane_seq: 0
collision: [v7-reader]
closed: 2026-08-28
closed_by: codex
commits:
- hash: 0a477a098
  summary: 'v7: finish root datum reader'
- hash: 392aa5521
  summary: 'v7: fix symbol diagnostic arity'
---

# Finish DL7 root datum reader

## Description

Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`, milestone 1.

Complete the existing reader's root datum contract. Bare atoms remain
unresolved syntax, `?name` remains a reader variable identity, and `'name`
becomes literal symbol data. Preserve empty and nested forms.

## Acceptance Criteria

- [x] `'name` reads as `literal(symbol(Name))` with an exact source span.
- [x] Bare `Name` remains `atom(Name)`.
- [x] Equal named variables share identity inside one top-level form.
- [x] Empty and nested forms retain their existing canonical shape.
- [x] Existing reader snapshot is extended without adding a test file.
- [x] `v7/3_TASKS/00_PROGRESS.md` records the receipt.

## Tests Run

- [x] Focused SWI reader test command from the worker brief.

## Implementation Notes

Worker brief: `v7/3_TASKS/12_ROOT_DATUM.GLM53F.BRIEF.md`.

## Resolution

### 2026-08-29T00:41:56Z · @codex

Reviewed both commits. Four focused reader tests pass; git diff --check is clean. The invalid-symbol diagnostic arity defect found during review is covered by the existing test file.
