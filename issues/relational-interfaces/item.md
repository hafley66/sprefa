---
created: 2026-08-18
updated: 2026-08-18
type: task
status: done
priority: normal
epic: relational-types
labels:
- area:dl6
- intent:type-system
assignee: terra
closed: 2026-08-18
closed_by: codex
commits:
- hash: dca9e788a
  summary: lower interface bounds through compile relations
---

# Lower interfaces through compile-time relations

## Description

Plan: plans/2026-08-18-relational-interfaces-and-result-arrows.md

## Acceptance Criteria

- [x] Existing interface, bound, and is syntax lowers to compile-time relation declarations, facts, and queries.
- [x] Compile-time type rows never enter runtime storage.
- [x] Derived conformance and missing-bound diagnostics have compiler tests.
- [x] Existing interface programs retain equivalent runtime output.

## Tests Run

## Implementation Notes

Preserve the current source surface. Replace the dedicated proof judge behind it.

## Resolution

### 2026-08-18T14:32:51Z · @codex

Integrated on main. Combined expansion_order and rel_template_and_is_clause CI passed 106/106; proof-plane erasure and recursive structural conformance are covered.
