---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: in-progress
priority: high
epic: type-surface-gaps
labels:
- area:dl6
- intent:syntax
blocked_by: ['@canonical-type-reflection']
---

# Accept native JSON literals

## Description

Accept copy-pasted JSON objects and arrays as a distinct literal AST and lower them to the existing canonical JSON value representation.

Plan: `v6/plans/2026-08-20-type-surface-gaps.md`.

## Acceptance Criteria

- [ ] Quoted-key objects and arrays parse as a distinct JSON-literal AST.
- [ ] Strings, integers, floats, booleans, null, empty values, and nesting work.
- [ ] JSON literals do not change object-pattern or relation-value braces.
- [ ] Object keys canonicalize and array positions remain ordered.
- [ ] Prolog, SQLite, TypeScript, and Rust round-trip identical JSON values.
- [ ] Printer and Tree-sitter coverage accept copy-pasted JSON.

## Tests Run

Pending.

## Implementation Notes

Lower into the existing JSON value representation and JSON1/serde boundaries.
