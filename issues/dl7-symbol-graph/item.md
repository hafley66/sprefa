---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: codex
status: done
priority: high
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f, model-codex]
size: M
lane: dl7-kernel
lane_seq: 0
collision: [v7-kernel, v7-comptime]
blocked_by: ['@dl7-datalog-checks']
closed: 2026-08-29
commits:
- hash: 3fa817fd3
  summary: finish checked Datalog basement
---

# Lower DL7 nodes, edges, products, sums, facts, and rules

## Description

Lower reader forms into one ground graph and positive Datalog IR. A file owns
one compiler-created module node. Nested products and sums mint nodes with
ordinary classifier rows. Every `:` bind becomes an ordered owner-name-target
edge. Facts and rules use the same positional call representation regardless
of whether their arguments later carry types or runtime values.

## Signatures

```prolog
lower_datalog(+Unit, -Program, -Origins, -Diagnostics).
check_datalog(+Program, +Origins, -Checked, -Diagnostics).
```

## Acceptance Criteria

- [x] Every identity has a `node(Id)` row and applicable classifier rows.
- [x] Canonical edges are `':'(Owner, Name, Target, Index)`.
- [x] `(Owner, Name)` and `(Owner, Index)` collisions are diagnosed.
- [x] Nested bind targets provide lexical parent lookup without a scope kind.
- [x] Facts and rules share `call(ref(Relation), Arguments)` lowering.
- [x] Type-valued and runtime-valued calls use the same lowering predicate.
- [x] No member vocabulary, synthetic public edge ID, or relation inference is added.
- [x] Production code lives in `v7/src/2_comptime/0_compiler.pl`.
- [x] No standalone test file is added.

## Tests Run

- [x] Direct nested-graph, recursive-rule, invalid-call, and edge-key receipts pass.

## Implementation Notes

The output is checked compiler data consumed by
`v7/src/1_libtime/0_evaluator.pl` and retained for later emitters.

## Resolution

### 2026-08-29T04:06:15Z · @issuectl

The landed compiler emits node and classifier rows, canonical colon edges, checked calls, dependencies, and deterministic key diagnostics through one lowering path.
