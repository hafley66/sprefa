---
created: 2026-08-19
updated: 2026-08-19
type: feature
status: done
closed: 2026-08-19
priority: high
epic: applicative-type-annotations
labels:
- area:language
- pkg:prolog
- pkg:extract
- codex
assignee: codex
---

# Type annotation surface and elaboration

## Description

Parse, print, CST-model, and elaborate direct compiler-relation applications
such as `key(int)` and `second(first(int))`. Plan:
`plans/2026-08-19-applicative-type-annotations.md`.

## Acceptance Criteria

- [x] Compiler relations returning `type` are accepted in type position.
- [x] Parse-print-reparse and CST CI cover plain, direct, configured, and nested forms.
- [x] Nested applications evaluate inside-out.

## Tests Run

- `annotation_surface`, `anonymous_type_syntax`, `type_relation_ir`: 66/66.
- `cargo test` in `tree-sitter-dl6`: 5/5.

## Implementation Notes

- `69d1bff6a`: parser, printer, CST, and structural elaboration.
- `72d78a3d5`: post-expansion compiler-plane handoff.
- `df6a54377`: recursive nested-site discovery and deduplication.
