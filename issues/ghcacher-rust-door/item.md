---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: in-progress
priority: normal
epic: cheap-fast-analysis
labels: [engine, ghcacher, lane-b]
---

# ghcacher on the Rust door with linked executors

## Description

Six tsv2 goldens gated through emit_rust_harness with scripted responses; executors fetch (ureq, ETag/304), env, gh_repos, soopy checkout, toml; one capped live smoke against cli/cli; cost sheet. Lane B.
