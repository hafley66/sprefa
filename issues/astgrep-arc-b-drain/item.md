---
created: 2026-08-25
updated: 2026-08-25
type: task
reporter: hafley66
status: testing
priority: high
epic: extract-astgrep-soopy
labels:
- pkg:extract
---

# Arc B: ast-grep Edit drains into soopy SourceAction, Act deleted

## Description

Plan section: PLAN.md '## Arc B'. From<Edit<String>> for soopy::TextEdit; PendingReplaceDoc whose do_edit appends to a pending SourceAction::Replace with expected ContentId instead of mutating the string; Plan.stages becomes Vec<Vec<soopy::SourceAction>>; Act and action() deleted from 0_move.rs. Owns: src/0_move.rs, src/drain.rs (new), tests/1_move.rs. Forbidden: src/lang/** (Arc A owns it), Cargo.toml. Gate: cargo test --release --features cli --test 1_move --test 0_prolog, byte-identical dry-run output on the 1_move fixtures.
