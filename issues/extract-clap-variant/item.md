---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# extract: clap variant names as a type-family record

## Description

rust.rs:636-651 pushes enum variants as TypeEdgeKind::Variant candidates whose to is synthetic text; nothing surfaces them. extract --family type on boop main.rs: 29 enum nodes, 0 variant rows. Want record=variant family=type owner span, owner_name, name. Blocks keying feature on the subcommand.
