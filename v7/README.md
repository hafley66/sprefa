# Sprefa V7

V7 is a fresh `.dl7` language and SWI-Prolog compiler. DL6 is a donor corpus for
semantic predicates, tests, rulings, and execution-plan contracts.

The first arc is a predicate-level DL6 reuse audit. Its receipts live under
`1_AUDIT/`. Source compatibility and a second maintained DL6 frontend are
outside this arc.

- [Donor audit index](1_AUDIT/results/0_INDEX.md)
- [Kernel reconciliation](2_DESIGN/0_KERNEL_RECONCILIATION.md)
- [Minimal programmable kernel plan](2_DESIGN/1_MINIMAL_VERTICAL_SLICE.PLAN.md)

Initial boundary under examination:

```text
.dl7 source
    -> generated Tree-sitter C parser
    -> canonical syntax adapter
    -> evaluator
    -> V7 semantic facts and fixpoints
    -> execution-plan contract
    -> existing sprefa-engine-rs
```

The first build gate is `cd v7 && just build`. It regenerates and tests the
DL7 Tree-sitter parser. The generated parser exposes a C ABI usable from C,
C++, Zig, and a later compiler-host adapter.
