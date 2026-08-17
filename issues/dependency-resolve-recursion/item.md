---
created: 2026-08-14
updated: 2026-08-16
type: feature
status: done
priority: high
epic: v5-behavioral-parity
labels: [parity, v6]
closed: 2026-08-16
commits:
- hash: e2f49f27
  summary: dep_resolve module; frontier closes on grafana corpus; local-only, acquisition deferred to policy issue
---

# Recursive dependency resolution from source_specifier.target to repository coordinates

## Description

## Goal
Resolve dependencies recursively from source_specifier.target to repository coordinates and enumerate their files until the frontier closes.
Works today:
- repo/rev/file -> source bytes -> source_specifier(owner span, target, binding, kind) -> dependency row
Remains:
- dependency target -> resolve package/module to repository -> acquire or locate repository -> select revision -> enumerate its files -> repeat until the frontier closes
## Where to put it
- New module under v6/sprefa-engine-rs/src/ (e.g. dep_resolve.rs) — keep it a sibling concern module, not a source_bind grow.
- SourceBind _1_runtime.rs emits specifier rows; this module walks them to repo coordinates.
## Perf gate
- v6/justfile: just crawl-bench (recursive crawl under nice -n 19)
- v6/justfile: just multirepo-golden
## Implementation Notes
Termination: the frontier must close. Guard against cycles and unbounded growth before it runs unattended.
