---
created: 2026-08-14
updated: 2026-08-16
type: task
status: open
priority: high
epic: v5-behavioral-parity
labels:
- parity
- v6
- size:med
lane: engine-source-bind
lane_seq: 30
collision: [source-bind-runtime, engine-driver]
---

# Watcher events auto-enter the engine tick schedule

## Description

## Goal
Watcher events enter the engine tick schedule automatically, so worktree/commit edits drive SourceBind arrivals with no manual restart. Parity with V5's live watcher.
## Where to put it
- v6/sprefa-engine-rs/src/source_bind/_1_runtime.rs — the SourceBind runtime tick path (397 lines; split if this grows the file past ~500).
- v6/sprefa-engine-rs/src/hosts.rs — follow the SoopyFilesExecutor pattern (name + execution tag match, no child spawn).
- v6/sprefa-engine-rs/src/driver.rs — the tick schedule the watcher feeds.
Keep AGENTS.md laws: no god files (split past ~500 lines), one concern per module.
## Perf gate
- v6/justfile: just watch-scale (coalescing correctness + cost, 100/1000 files)
- v6/justfile: just scale-floor (arrivals at scale, stmts/tick flat)
## Implementation Notes
Duplicates / identical re-saves / deletes must coalesce; correctness gated, cost reported (watch-scale does not gate wall ms).
