---
created: 2026-08-30
updated: 2026-08-30
type: task
status: open
priority: high
epic: dl7-expression-flow
labels: [compiler]
size: M
lane: dl7-expression-flow
lane_seq: 3
collision: [v7-lowerer, v7-test]
blocked_by: ['@dl7-bind-applications']
---

# Flatten nested DL7 applications

## Description

Recursively lower nested relation applications into ordered flat goals with shared fresh variables. Model class: Luna.

## Acceptance Criteria

- [ ] `(Option (Partial User))` produces two flat ordered goals.
- [ ] The inner result variable is the outer input variable.
- [ ] Nested diagnostics point at the originating reader node.

## Tests Run

- [ ] Focused V7 SWI tests pass.

## Implementation Notes

Plan milestone 4. Keep the checked goal representation first-order.
