---
created: 2026-08-29
updated: 2026-08-29
type: epic
owner: codex
status: open
priority: normal
labels: [dl7, engine]
---

# DL7 engine artifact adapter

## Description

Project V7 checked Datalog through a target-neutral layout plan into the existing ProgramJson and generated Rust module door. The existing Rust engine remains unchanged.

## Acceptance Criteria

- [ ] A bounded V7 layout graph names physical relations, columns, keys, and statements.
- [ ] ProgramJson fields derive from the layout graph.
- [ ] The existing Rust loader consumes one generated V7 artifact.
- [ ] No V6 parser terms cross the adapter.
- [ ] No Rust engine source changes.

## Tests Run

- [ ] One exact engine smoke command.

## Implementation Notes

Contract receipt: v7/tasks/results/9_ENGINE_SEAM.md.
