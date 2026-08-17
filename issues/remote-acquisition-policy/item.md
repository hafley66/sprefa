---
created: 2026-08-14
updated: 2026-08-14
type: feature
status: open
priority: normal
epic: v5-behavioral-parity
labels: [parity, v6]
---

# Remote acquisition and checkout policy for discovered repositories

## Description

## Goal
Remote acquisition and checkout policy for newly discovered repositories during dependency crawling: locate an existing checkout or acquire+checkout the remote, selecting a revision.
## Where to put it
- New module under v6/sprefa-engine-rs/src/ (e.g. acquisition.rs) — sibling concern module, not a source_bind grow.
- Policy (locate-vs-fetch, revision selection, checkout depth) in the sprefa config surface, not hardcoded.
## Perf gate
- v6/justfile: just crawl-bench
## Implementation Notes
Acquisition is network + disk bound; gate on the crawl receipt, and keep the policy configurable so a run against the pinned corpus is hermetic (no live fetch in green).
