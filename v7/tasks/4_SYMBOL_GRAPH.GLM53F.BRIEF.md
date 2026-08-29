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

- [ ] Every identity has a `node(Id)` row and applicable classifier rows.
- [ ] Canonical edges are `':'(Owner, Name, Target, Index)`.
- [ ] `(Owner, Name)` and `(Owner, Index)` collisions are diagnosed.
- [ ] Nested bind targets provide lexical parent lookup without a scope kind.
- [ ] Facts and rules share `call(ref(Relation), Arguments)` lowering.
- [ ] Type-valued and runtime-valued calls use the same lowering predicate.
- [ ] No member vocabulary, synthetic public edge ID, or relation inference is added.
- [ ] Production code lives in `v7/src/2_comptime/0_compiler.pl`.
- [ ] No standalone test file is added.

## Tests Run

- [ ] Direct nested-graph, recursive-rule, invalid-call, and edge-key receipts pass.

## Implementation Notes

The output is checked compiler data consumed by
`v7/src/1_libtime/0_evaluator.pl` and retained for later emitters.
