---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: open
priority: high
epic: type-surface-gaps
labels:
- area:dl6
- intent:syntax
blocked_by: ['@canonical-type-reflection']
---

# Accept inline arrow type expressions

## Description

Accept arrows in every recursive type-expression position and lower each to the existing anonymous relation machinery with an ordinary return member.

Plan: `v6/plans/2026-08-20-type-surface-gaps.md`.

## Acceptance Criteria

- [ ] `type_expr` accepts `((inputs) -> Output)` recursively.
- [ ] Field, wrapper, generic argument, product, and sum sites compile.
- [ ] The minted anonymous relation has one ordinary `return` member role.
- [ ] Equivalent named and inline relations produce equal canonical graphs.
- [ ] Prolog printer and Tree-sitter parse-print coverage pass.
- [ ] TypeScript, Rust, and JSON Schema artifacts expose the inline type.

## Tests Run

Pending.

## Implementation Notes

Reuse anonymous relation minting and storage. Add no operation/service special
form.
