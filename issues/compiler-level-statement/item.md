---
created: 2026-08-21
updated: 2026-08-21
type: improvement
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
---

# compiler: level_statement_groups and the emitter's format/3 calls

## Description

lower:level_statement_groups 429,849 inf / 39ms is the largest single step; emit makes 5,459 format/3 calls, 23% of profiler self-ticks, C work invisible in inference counts. parse_source is 25 inferences per input character. Two generic pipelines run with identical input and output on the rail; eliding the second is language design.
