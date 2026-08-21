---
created: 2026-08-21
updated: 2026-08-21
type: feature
reporter: chris
assignee: chris
status: done
priority: normal
epic: cheap-fast-analysis
labels: [engine, ghcacher, lane-b]
closed: 2026-08-21
closed_by: chris
commits:
- hash: 779e90286
  summary: ghcacher on the Rust door, six goldens, ureq fetch with 304
---

# ghcacher on the Rust door with linked executors

## Description

Six tsv2 goldens gated through emit_rust_harness with scripted responses; executors fetch (ureq, ETag/304), env, gh_repos, soopy checkout, toml; one capped live smoke against cli/cli; cost sheet. Lane B.
