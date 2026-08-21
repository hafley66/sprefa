---
created: 2026-08-21
updated: 2026-08-21
type: task
reporter: chris
assignee: chris
status: done
priority: normal
epic: cheap-fast-analysis
labels: [engine, lane-a]
closed: 2026-08-21
closed_by: chris
commits:
- hash: d011ddc77
  summary: landed via lane A
---

# Delete ShellExecutor from sprefa-engine-rs

## Description

Every sh host links a Rust executor (soopy_files, sprefa_extract, sprefa_scip, cargo_metadata, fixture) or is a named stop at construction. No sh -c anywhere in the engine. Lane A.
