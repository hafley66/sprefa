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

v5 f859585ed. Rust impl only. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: fixture with a private fn used by a sibling, cargo check green
