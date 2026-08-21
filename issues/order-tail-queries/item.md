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
- hash: ba2daa779
  summary: order by tail on ? queries, lowered onto the final cursor
---

# order by tail on ? queries, lowered onto the final cursor

## Description

Grammar tail, ORDER BY on final_select (Rust door), covering index when the read hits the base table, EXPLAIN pinned; rail drops its bash sort.
