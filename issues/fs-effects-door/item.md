---
created: 2026-08-15
updated: 2026-08-16
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

## Comments

### 2026-08-17T02:54:18Z · @coordinator

Validated 2026-08-16: the WRITE-side mechanics are soopy's and BUILT — stage (crates/soopy/src/_7e_stage_store.rs: stage_mutations, StageId, canonical manifest) and commit/recover with journal (_7f_commit.rs:186,263). plans/2026-08-16-soopy-stage-commit-source-actions.RESEARCH.md fixes the boundary: soopy validates/previews/stages/commits, DL6 owns relational policy (which rel shape is a write, conflicts, approval fact releasing a staged digest). Remaining scope: DL6 rel-shape + capability design and emit glue to SourceAction rows; the rel-shape half is lang design, Chris in the room.
