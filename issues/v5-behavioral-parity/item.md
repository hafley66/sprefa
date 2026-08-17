---
created: 2026-08-14
updated: 2026-08-14
type: epic
owner: chrishafley
status: open
priority: high
---

# V5 behavioral parity for the V6 engine

## Description

Source-coordinate and extraction-boundary parity with V5 is in place (repo/rev/file/span/source_specifier authored; sprefa-extract emits specifiers; SourceBind ticks arrivals). This epic closes the remaining V5 behavioral gaps: watcher auto-tick, DL6 change/span/git-ref relations, recursive dependency resolution, remote acquisition, restart-safe retraction, and the end-to-end V5 source workload ports. Every child carries a justfile perf gate and a code-placement note so the v6 tree stays trim.
