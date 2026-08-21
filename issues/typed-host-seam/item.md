---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: in-progress
priority: normal
epic: cheap-fast-analysis
labels: [engine, perf, lane-a]
---

# Typed host seam: executors return rows, never JSON text

## Description

SprefaExtractExecutor serializes facts to JSONL and decode_output re-parses per declaring host; host_collect is 599ms of a 1214ms rail run with zero SQL. Convert once per group, project by column presence. Lane A.
