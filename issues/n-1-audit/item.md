---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: in-progress
priority: normal
epic: cheap-fast-analysis
labels: [engine, extract, perf, lane-d]
---

# N+1 audit of engine wiring and extract with COUNT tests

## Description

Every find/contains/position in a loop, every SQL in a for-row loop, key() replace path, recount per call, intern, ticklog, call_facts per-edge find, probe per specifier. Red COUNT test first, fix in owned files, diffs for the rest. Lane D.
