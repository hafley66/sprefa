# SCIP-SPEED.REPORT.md the scip-informed leg under the 10-second law (2026-08-30)

Lane: `fix-extract-scip-speed-2`. Base: origin/main 619724c7f merged with
origin/fix/extract-scip-speed. Binary:
`v6/sprefa-extract/target/release/extract`.

## Contents

1. What the inherited commits already cover
2. Task A: index the index
3. Task B: go scip rows against the vta bare oracle
4. Task C: informed-by-default when the index is fresh

## 1. What the inherited commits already cover

| commit | what it is | Task A | Task B | Task C |
|---|---|---|---|---|
| 4084f4abb | the family stream emits the raw `scip_relationship` table (override flags ride the wire) | no | no | no |
| 32c016572 | one-process measurement, gap classification, SCIP.REPORT.md | no (it measured the slow seam, 113/163/436 s) | no | no |
| 6e1632824 | test file number + report title | no | no | no |
| 309837c60 | merge of the above into the lane branch | no | no | no |

None of the four inherited commits is speed work: Task A (index the index),
Task B (go rows against `go.oracle.call.vta.bare.tsv`) and Task C
(informed-by-default when fresh) all start at zero in this lane. The
inherited walls (one process, prior lane): ts 113 s, rust 163 s, go 436 s.
