---
created: 2026-08-21
updated: 2026-08-21
type: bug
reporter: chris
assignee: chris
status: in-progress
priority: normal
epic: cheap-fast-analysis
labels: [extract, perf, lane-a]
---

# join_documents runs once per file: resolve over 82 files never finishes

## Description

lang/rust.rs:978, go.rs:1933, ts.rs:3168 call join_documents(index, reader) per file; 82 x 129 whole-corpus reads, killed at 506s with 0 rows. OnceLock on IndexBag. Lane A.
