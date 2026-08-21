---
created: 2026-08-21
updated: 2026-08-21
type: bug
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# boop: plan/ worktrees lack the hafley-rs and sprefa-v6 symlinks feature/ gets

## Description

boop beep lane create --branch plan/<name> creates .boop-worktrees/plan/<name> but the sibling symlinks hafley-rs and sprefa-v6 that .boop-worktrees/feature/ carries are absent, so no Rust crate builds in a plan/ worktree until someone creates them by hand (the v5 census lane did). The spawner should create them per kind dir. hafley-rs boop work.
