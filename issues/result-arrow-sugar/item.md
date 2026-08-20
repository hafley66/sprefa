---
created: 2026-08-18
updated: 2026-08-18
type: task
status: done
priority: high
epic: relational-types
labels:
- area:dl6
- intent:syntax
assignee: luna
closed: 2026-08-18
closed_by: codex
commits:
- hash: 710eef0e5
  summary: add relation result arrow sugar
---

# Use Result through relation arrow sugar

## Description

Plan: plans/2026-08-18-relational-interfaces-and-result-arrows.md

## Acceptance Criteria

- [x] rel F(inputs) -> OutputType lowers to an ordinary final `return` column.
- [x] Rule heads retain ordinary F(inputs, Return) <- Body syntax.
- [x] Input/return name collision has a named compiler refusal.
- [x] Result(Error, Value) appears in the comprehensive DL6 golden.
- [x] Explicit and arrow forms match in compiler and emitted-runtime CI.
- [x] TypeScript, Rust, and JSON Schema output passes existing type-generation CI.

## Tests Run

## Implementation Notes

The arrow is declaration-only sugar. `return` is an ordinary DL6 identifier and Rust emits it as `r#return`. One output value only. Product or sum relations carry multiple logical outputs. No host or effect semantics.

## Resolution

### 2026-08-18T14:32:51Z · @codex

Integrated on main. Combined compiler CI passed 106/106 and comprehensive golden coverage passed 83/83.
