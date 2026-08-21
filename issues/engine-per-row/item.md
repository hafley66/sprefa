---
created: 2026-08-21
updated: 2026-08-21
type: improvement
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
labels: [engine, perf]
---

# Engine per-row cost: 0.97s for 54k rows on sf_join

## Description

After the five quadratic loops, SQLite is 68% of the wall: stage, publish, recount dominate. Next: fewer statements per tick is not the lever (measured); row volume per verb is. Profile with DL_TRACE_SUMMARY.
