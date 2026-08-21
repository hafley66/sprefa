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
- hash: 77b4b539e
  summary: Engine tracing and the measured SQLite seam
---

# Engine tracing and the measured SQLite seam

## Description

Per verb/relation/statement spans, DL_TRACE_SUMMARY; five quadratic dedup loops indexed: 54k rows 5.4s to 0.97s; statement cache, limit from connection, pragmas measured.
