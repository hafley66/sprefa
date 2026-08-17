---
created: 2026-08-16
updated: 2026-08-16
type: task
status: open
priority: normal
epic: joern-striking-distance
labels:
- pkg:extract,size:small
---

# research: Joern CPG spec inventory, tree-sitter-graph, CPG protobuf import, kind_role census

## Description

## Comments

### 2026-08-16T17:27:07Z · @fable

READ-ONLY research, no code. Deliverable: plans/2026-08-16-cpg-spec-research.REPORT.md. Four questions, each with citations: (1) the published Joern CPG spec's exact node and edge vocabulary (enumerate every edge kind with its semantic, from the spec, not from memory); (2) tree-sitter-graph: can its per-lang .tsg rules express the kind_role mapping (branch/loop/jump roles per CST kind), and what its runtime costs; (3) the CPG protobuf schema: feasibility + shape of an importer alongside our SCIP importer (scip_decode.rs pattern); (4) kind_role census: for rust/go/kotlin/ts grammars, enumerate the CST kind names that are branch, loop, jump, exit (grep the grammars under v6/sprefa-extract deps or node kinds via the existing CstF output). Anchor doc: plans/2026-08-16-joern-cpg-striking-distance.md.
