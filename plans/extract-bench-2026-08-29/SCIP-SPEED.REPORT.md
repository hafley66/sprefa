# SCIP-SPEED.REPORT.md the scip-informed leg under the 10-second law (2026-08-30)

Lane: `fix-extract-scip-speed-3`. Base: 292888ba5 (ff from origin/main) merged
with origin/fix/extract-scip-speed, plus the dead-lane scip.rs patch applied as
017e23d3c. Binary: `v6/sprefa-extract/target/release/extract`.

## Contents

1. What the inherited commits and the recovered patch already cover
2. Task A: index the index (walls, before/after jsonl identity)
3. Task B: go scip rows against the vta bare oracle
4. Task C: informed-by-default when the index is fresh

## 1. What the inherited commits and the recovered patch already cover

| commit | what it is | Task A | Task B | Task C |
|---|---|---|---|---|
| 4084f4abb | the family stream emits the raw `scip_relationship` table (override flags ride the wire) | no | no | no |
| 32c016572 | one-process measurement, gap classification, SCIP.REPORT.md | no (it measured the slow seam, 113/163/436 s) | no | no |
| 6e1632824 | test file number + report title | no | no | no |
| 017e23d3c | scip.rs patch recovered from the killed lane: per-document sorted occurrence vectors, symbol->def map, address+fingerprint-keyed caches | **yes (the implementation)** | no | no |

The inherited commits are measurement and wire work only. The recovered patch
implements Task A's caching seam in `src/scip.rs`: `doc_cache` builds one
sorted `(start, end, occ_ix)` span vector plus a shared `LineTable` per
document, so `site_occurrence` binary-searches (`partition_point`) instead of
scanning every occurrence per site; `def_map` interns one `symbol -> (doc_ix,
occ_ix)` first-definition map per index, so `definition_of` for global symbols
is one lookup. `local ` symbols stay document-scoped per the SCIP convention.
Task B and Task C start at zero in this lane. The pre-fix walls (one process,
prior lane): ts 113 s, rust 163 s, go 436 s.
