---
created: 2026-08-16
updated: 2026-08-16
type: chore
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# One resolution of soopy transitives across the two lockfiles

## Description

extract and engine workspaces resolve soopy's ignore to 0.4.31 vs 0.4.33 (also blake3, globset, clap): ignore::WalkBuilder decides worktree membership (_4_worktree.rs:36), so two builds of one soopy source can disagree on which files exist. Unify workspaces or pin. Candidate 13.
