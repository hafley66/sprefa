---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: done
priority: normal
epic: cheap-fast-analysis
closed: 2026-08-21
closed_by: chris
commits:
- hash: a104dc5f4
  summary: Extract tracing and compiler step trace
---

# Extract tracing and compiler step trace

## Description

Spans on every parse/family/resolve/cache/flatten; DL6_TRACE=steps names 25 plan steps and 21 lower/boot steps.
