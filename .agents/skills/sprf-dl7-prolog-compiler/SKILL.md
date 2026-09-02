---
name: sprf-dl7-prolog-compiler
description: Work on Sprefa DL7's SWI-Prolog compiler, Datalog evaluator, comptime fixpoint, demand relations, or compiler performance without rediscovering established semantics and failed optimization paths.
---

# DL7 Prolog compiler

Use this skill for changes under `v7/src/1_libtime` or `v7/src/2_comptime`,
for compiler-fixpoint performance work, and when translating between Prolog
execution behavior and DL7's checked Datalog semantics.

Read:

- [references/0_semantic_model.md](references/0_semantic_model.md) before
  changing evaluation order, snapshot visibility, closure publication,
  tabling, negation, aggregation, or the `nil`/`cons`/`intern` relations.
- [references/1_performance_model.md](references/1_performance_model.md) before
  changing evaluator storage, invalidation, caching, incremental evaluation,
  compiler rounds, or performance gates.
- [references/2_cst_extract_pipeline.md](references/2_cst_extract_pipeline.md)
  before connecting DL7 to Tree-sitter, ast-grep, `sprefa-extract`, external
  schema ingestion, or the DBSP application emitter.
- [references/3_relational_execution_ir.md](references/3_relational_execution_ir.md)
  before changing compile-time versus runtime extraction, the logical-to-
  physical plan boundary, SQLite DBSP lowering, or target runtime artifacts.

## Working rule

Preserve new evidence-backed Prolog and DL7-specific compiler insights in the
appropriate numbered reference. Record the measured fixture, command, row or
inference counts, and semantic result. Keep hypotheses in plans until a lab or
test establishes them.

For evaluator optimizations, retain an exact full-snapshot oracle mode. Compare
complete sorted closures and diagnostics, rather than row counts alone.
