---
created: 2026-08-16
updated: 2026-08-16
type: task
reporter: chris
status: done
priority: high
epic: soopy-full-wiring
closed: 2026-08-16
---

# Revive SourceTreeBlobSource; retire FsBlobSource from production

## Description

FsBlobSource (std::fs::read, no revision, project.rs:691) serves both production readers (project.rs:147,195); rev-correct SourceTreeBlobSource is tested (tests/10_source_tree.rs:42-57) with zero callers. Swap the two call sites, then route read_inputs corpus ingest (project.rs:379) through BlobSource with one batched read_each. Candidates 2+3.

## Comments

### 2026-08-17T02:58:00Z · @soopy-driver

VERIFIED LANDED at origin/main a4045153e (PR #309). project.rs:150,220,449 all construct SourceTreeBlobSource; FsBlobSource has zero production call sites (only project.rs:718 decl, :925 impl, :938 BlobSource impl, lib.rs:65 re-export, doc-commented test-only). read_inputs (project.rs:411) now forks discover->read_inputs_batched, with read_inputs_plain the no-repo fallback.
