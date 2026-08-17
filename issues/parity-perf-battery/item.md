---
created: 2026-08-14
updated: 2026-08-14
type: task
status: open
priority: normal
epic: v5-behavioral-parity
labels: [perf, v6]
---

# Parity perf battery wired into the justfile

## Description

## Goal
Perf testing for the V5-parity slice, run through the justfile so every new relation/host lands with a measured gate, not a pass-in-a-sandbox.
## Where to put it
- v6/labs/BENCHMARKS.md — document each new bench's purpose, budget, history (row-by-row, TOC).
- v6/justfile perf-all recipe — add each new leg under a named run-capped budget, echoing a ~10-word purpose header; a failing leg reports, does not abort.
## Gates to run per issue (v6/justfile unless noted)
- watcher-auto-tick: just watch-scale, just scale-floor
- dl6-change-facts: just precommit-changed
- span-line-column: just scale-floor
- dl6-git-ref-ancestry: just multirepo-golden, just v5-parity
- dependency-resolve-recursion: just crawl-bench, just multirepo-golden
- remote-acquisition-policy: just crawl-bench
- restart-safe-retraction: just dd-grade / just rust-grade (sprefa-engine-rs), just memory-soak
- v5-source-workload-ports: just v5-parity, just multirepo-golden
## Implementation Notes
Consolidated battery is v6/justfile just perf-all (and perf-all-deep for the full store ladder). Budgets are deliberately loose; the gated part is correctness and delta-proportionality, wall is informational.
