---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: done
priority: normal
epic: cheap-fast-analysis
labels: [engine, extract, perf, lane-d]
closed: 2026-08-21
closed_by: chris
commits:
- hash: b3a3cd7e8
  summary: 18 quadratics fixed with COUNT tests; sf_join 0.97 to 0.85s; scip resolve 65 files 150s+ to 4.7s
---

# N+1 audit of engine wiring and extract with COUNT tests

## Description

Every find/contains/position in a loop, every SQL in a for-row loop, key() replace path, recount per call, intern, ticklog, call_facts per-edge find, probe per specifier. Red COUNT test first, fix in owned files, diffs for the rest. Lane D.
