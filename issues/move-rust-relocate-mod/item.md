---
created: 2026-08-27
updated: 2026-08-27
type: feature
assignee: chris
status: open
priority: high
epic: extract-move-parity
labels: [extract, refactor, rust]
---

# extract move --relocate-mod: Rust use-path re-pathing when a module changes parent

## Description

v5 src/rspath.rs + f859585ed mod surgery, v1 crates/rs/src/lib.rs:270. Second strategy in RustSource::respell behind a flag; default stays #[path]. Plan: plans/2026-08-27-extract-move-parity-v1-v5.PLAN.md. Receipt: fixture: use crate::a::f -> use crate::util::a::f, cargo check green, tests/3_move_rust.rs
