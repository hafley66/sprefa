---
created: 2026-08-30
updated: 2026-08-30
type: task
status: done
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: M
lane: dl7-expression-flow
lane_seq: 3
collision: [v7-lowerer, v7-test]
blocked_by: ['@dl7-bind-applications']
closed: 2026-08-30
closed_by: codex-0
commits:
- hash: a7bf8c2ef
  summary: Flatten nested DL7 relation expressions
- hash: 1636e9795
  summary: Prove nested Partial and Option application
- hash: 04f4f010b
  summary: Pin nested expression diagnostics
---

# Flatten nested DL7 applications

## Description

Recursively lower nested relation applications into ordered flat goals with shared fresh variables. Model class: Luna.

## Acceptance Criteria

- [x] `(Option (Partial User))` produces two flat ordered goals.
- [x] The inner result variable is the outer input variable.
- [x] Nested diagnostics point at the originating reader node.

## Tests Run

- [x] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 4. Keep the checked goal representation first-order.

## Resolution

### 2026-08-30T21:34:45Z · @codex-0

Nested and chained applications flatten to first-order goals with positioned inner diagnostics. Complete V7 SWI passed 19 of 19 and Tree-sitter passed 1 of 1.
