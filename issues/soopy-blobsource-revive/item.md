---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: open
priority: high
epic: soopy-full-wiring
---

# Revive SourceTreeBlobSource; retire FsBlobSource from production

## Description

FsBlobSource (std::fs::read, no revision, project.rs:691) serves both production readers (project.rs:147,195); rev-correct SourceTreeBlobSource is tested (tests/10_source_tree.rs:42-57) with zero callers. Swap the two call sites, then route read_inputs corpus ingest (project.rs:379) through BlobSource with one batched read_each. Candidates 2+3.
