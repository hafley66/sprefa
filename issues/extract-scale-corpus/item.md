---
created: 2026-08-15
updated: 2026-08-15
type: task
reporter: fable
status: done
priority: normal
epic: bug-mining
labels:
- size:small
- area:extract
- bugmine
- pkg:extract
closed: 2026-08-15
commits:
- hash: e0b449539295f9d1332f48d15814273e19f659a2
  summary: scale-invariant sweep over tokio+rust-analyzer, filed df-empty-expr-zero-width
---

# Extractor invariants over external-scale corpora

## Description

Run extract over large third-party repos (tokio, rust-analyzer scale), assert per-file invariants: span containment in file bounds, no zero-width df nodes, no cross-kind span aliasing, edge endpoints resolve. Evidence: df-span-identity-aliasing screamed at 33 files; scale multiplies signal. Small and cheat-resistant: invariants are mechanical asserts over the wire output.
