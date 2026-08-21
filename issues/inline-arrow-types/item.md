---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: done
priority: high
epic: type-surface-gaps
labels:
- area:dl6
- intent:syntax
blocked_by: ['@canonical-type-reflection']
closed: 2026-08-20
---

# Accept inline arrow type expressions

## Description

Accept arrows in every recursive type-expression position and lower each to the existing anonymous relation machinery with an ordinary return member.

Plan: `v6/plans/2026-08-20-type-surface-gaps.md`.

## Acceptance Criteria

- [x] `type_expr` accepts `((inputs) -> Output)` recursively.
- [x] Field, wrapper, generic argument, product, and sum sites compile.
- [x] The minted anonymous relation has one ordinary `return` member role.
- [x] Equivalent named and inline relations produce equal canonical graphs.
- [x] Prolog printer and Tree-sitter parse-print coverage pass.
- [x] TypeScript, Rust, and JSON Schema artifacts expose the inline type.

## Tests Run

2026-08-20: `anonymous_type_syntax` 25/25. The fixture covers recursive
field, wrapper, generic argument, product, and sum sites; focused assertions
also compare named and inline member-role graphs and render TS, Rust, and JSON
Schema from the real `.dl6` fixture.

## Implementation Notes

Reuse anonymous relation minting and storage. Add no operation/service special
form.
