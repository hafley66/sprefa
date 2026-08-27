---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: normal
epic: extract-move-parity
labels: [extract, refactor, rust]
---

# extract move: private -> pub(crate) promotion when a moved item leaves its module scope

## Description

v5 f859585ed. Rust impl only. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md.

## Acceptance Criteria

- [x] private fn used by a sibling after relocation widens to pub(crate)
- [x] private fn used only inside its module stays private
- [x] already-pub items untouched; nothing widens past pub(crate)
- [x] without --relocate-mod nothing is widened
- [x] fixture cargo check green after --commit (receipt: oracle test, 1.13s)
