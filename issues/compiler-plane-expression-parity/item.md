---
created: 2026-08-24
updated: 2026-08-24
type: task
assignee: codex
status: done
priority: high
epic: userland-type-graph
labels:
- area:dl6
- area:compiler
- intent:semantics
- size:large
- model:large
lane: typegraph-core
lane_seq: 45
collision: [generic-type-core, compiler-oracle]
size: L
closed: 2026-08-24
closed_by: codex
commits:
- hash: 79ddb1925
  summary: 'compiler: evaluate expressions and grouped counts'
---

# Reuse normal expressions and aggregates in compiler rules

## Description

Allow authored compiler-plane DL6 rules to reuse the ordinary bounded scalar, ordering, comparison, and aggregate semantics needed by general type operators. Keep compiler rules finite, deterministic, storage-free, and erased before runtime planning.

## Required Signatures

Inventory the normal rule IR signatures first. Define the smallest shared evaluator boundary for integer arithmetic, ordering/comparison, grouped count, and complete finite-member checks. Pseudocode follows the signatures.

## Timeline

Expressions and aggregates evaluate inside one immutable compiler graph round. Their rows may create complete type-construction requests. New types become visible only after the existing bounded refreeze.

## Storage and Uniqueness

Compiler expression results use set semantics and create no runtime relation, SQLite table, boot row, arrival, or target payload. Grouped aggregate keys determine one result per complete group.

## Acceptance Criteria

- [x] Compiler and runtime scalar expressions share one semantic contract.
- [x] Integer addition can shift ordered member positions for user-land `concat`.
- [x] Ordering and comparison have deterministic ground-domain checks.
- [x] Finite grouped count supports all-members checks without open-world negation.
- [x] Aggregate inputs are complete within one immutable compiler round.
- [x] Construction results enter the existing bounded refreeze frontier.
- [x] Compiler expression and aggregate rows erase before runtime planning.
- [x] No type-operator-specific host builtin is added.

## Tests Run

Focused compiler evaluator, expression parity, aggregate determinism, refreeze, erasure, and complete PLUnit gates.

## Implementation Notes

Execution tier: Large. Current Codex owns the evaluator and semantic work directly. Any requirement for unbounded arithmetic, open-world negation, or cross-round partial aggregation yields for user direction.

## Agent Runs

### 2026-08-24T23:28:18Z · @codex

Read-only seam inventory at main fdfb80106. Normal contract rows: compile/registry.pl expression/5 plus expression_for_term/5 (31 registered expression signatures) and 12 aggregate surface signatures. Runtime SQL seam: lower.pl compile_expr/7. Analyzer classifiers: guard_goal/1, bind_goal/3, aggregate_head_template/2, rule_is_aggregate/1. Reference execution seams: conformance/body.pl eval_expr/2 and comparison_goal/1; conformance/level_eval.pl aggregate_head/3, agg_rule_rows/4, agg_compute/3. Current compiler evaluator only recognizes true, conjunction, type_apply/3, and relation-row membership; body_atoms/2 therefore classifies binds/comparisons as relations, and derive_compiler_row/3 requires a ground ordinary head. Proposed bounded signatures before implementation: eval_ground_expression(+Expr,-Value); holds_ground_comparison(+Goal); compiler_body_goals(+Body,-Goals); validate_compiler_goal_sequence(+DeclaredRefs,+Goals,+Bound0,-Bound); satisfy_compiler_goal(+Rows,+Goal); compiler_head_template(+Head,-plain_or_aggregate); evaluate_compiler_strata(+Rules,+Rows0,-Rows); derive_compiler_aggregate_row(+CompleteRows,+Rule,-Row). Timeline: relation joins bind first, := reads prior ground values then binds its LHS, comparisons read ground values, aggregate rules read a completed lower stratum once, and construction requests enter the existing refreeze frontier. Storage: sorted set rows only; aggregate uniqueness is the evaluated non-aggregate head tuple, one output per complete group; no runtime declarations or tables. Implementation must share registry classification and a production ground evaluator with the reference interpreter, add no type-operator builtin, and remain outside Terra dot-brace writes.

### 2026-08-25T03:23:04Z · @codex

Implemented shared reference-evaluator scalar binds and comparisons, authored-order ground validation, tabled positive strata, finite grouped count over completed lower strata, aggregate-cycle diagnostics, source elaboration, type-construction refreeze, and erasure coverage. Focused gate: 53/53 PLUnit tests passed. Code commit: 79ddb1925.

## Resolution

### 2026-08-25T03:23:09Z · @codex

All acceptance criteria are checked. The compiler plane now reuses the runtime reference scalar evaluator, computes count over completed strata, preserves tabling for positive closure, refreezes generated type requests, and erases compiler rows.
