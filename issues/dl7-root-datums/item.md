---
created: 2026-08-28
updated: 2026-08-28
type: task
assignee: glm53f
status: open
priority: high
epic: dl7-minimal-kernel
labels: [dl7, basement, model-glm53f]
size: S
lane: dl7-basement
lane_seq: 0
collision: [v7-reader]
---

# Finish DL7 root datum reader

## Description

Plan: `v7/2_DESIGN/2_BASEMENT_TO_DATALOG.PLAN.md`, milestone 1.

Complete the existing reader's root datum contract. Bare atoms remain
unresolved syntax, `?name` remains a reader variable identity, and `'name`
becomes literal symbol data. Preserve empty and nested forms.

## Acceptance Criteria

- [ ] `'name` reads as `literal(symbol(Name))` with an exact source span.
- [ ] Bare `Name` remains `atom(Name)`.
- [ ] Equal named variables share identity inside one top-level form.
- [ ] Empty and nested forms retain their existing canonical shape.
- [ ] Existing reader snapshot is extended without adding a test file.
- [ ] `v7/3_TASKS/00_PROGRESS.md` records the receipt.

## Tests Run

- [ ] Focused SWI reader test command from the worker brief.

## Implementation Notes

Worker brief: `v7/3_TASKS/12_ROOT_DATUM.GLM53F.BRIEF.md`.
