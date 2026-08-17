---
created: 2026-08-15
updated: 2026-08-16
type: task
reporter: fable
status: wontfix
priority: normal
epic: bug-mining
labels:
- size:small
- area:testing
- bugmine
- pkg:tsv2
closed: 2026-08-15
---

# Naive-vs-incremental self-diff over the existing corpus

## Description

SPREFA_TSV2_EMITTER_MODE=naive is a second oracle already built. Run every compiled corpus fixture through naive AND incremental on the same emitted module, byte-diff. This is exactly how the mutual-recursion probe proved the defect. Small and cheat-resistant: the gate is a byte diff over 339 fixtures, nothing to fudge.

## Resolution

### 2026-08-15T18:21:31Z · @issuectl

cancelled by design decision: the naive emitter mode is deleted (chore/delete-naive-arm), the selfdiff leg has no subject

## Comments

### 2026-08-16T17:03:05Z · @fable

Two defects measured from a live hang (Aug 15 run, killed Aug 16 after 23h): (1) the naive emitter arm has NO divergence bound: golden-run.ts on diverging_measure_recursion_is_bounded_and_loud spun 23h at 30% CPU with zero bytes in .nai.out/.nai.err; the fixture exists to prove the bound and instead proved its absence by hanging. (2) naive-selfdiff.sh's xargs worker has no per-fixture timeout, so one diverging fixture wedges the whole 12-way run forever; 11 workers finished, the run never exited. Both violate the 10-second law. Fix shape: a tick/step cap in the naive arm that exits loud, plus a per-fixture timeout in the worker.
