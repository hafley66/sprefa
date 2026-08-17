---
created: 2026-08-15
updated: 2026-08-15
type: feature
reporter: fable
status: open
priority: normal
epic: bug-mining
labels:
- size:med
- area:testing
- bugmine
- pkg:prolog
- pkg:tsv2
- pkg:engine-rs
---

# Arrival-schedule permutation fuzzing

## Description

Same program, shuffled arrival batching/order, assert final-state equality across schedules and doors. golden-schedules.ts has 4 fixed schedules today; randomize batching with a seed, minimize on divergence. Targets the tick-phase/retraction-order class (ledger 49-52).
