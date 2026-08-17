---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: open
priority: normal
epic: dl6-first-typegen
labels:
- size:large
- area:engine
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
---

# fs-effects door: dl6 writes files

## Description

Queued arc since Phase F (file writing deliberately out of scope there; the dataflow rail's wrapper owns its write today, said in report-extract.sh's header). Design the effect boundary: which rel shape marks a file write, both doors byte-identical, no JS-side row engine, capability-gated. Large: it is an effects-model design, needs trade-off weighing and probably a decision row.
