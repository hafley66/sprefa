---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: terra
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@interface-bound-transport', '@semantic-type-identity', '@type-relation-ir']
lane: type-core
lane_seq: 3
collision: [generic-type-core, compiler-oracle]
closed: 2026-08-18
commits:
- hash: f5bda1321
  summary: 'compiler: evaluate type-valued relations'
---

# Evaluate type-valued compiler relations

## Description

Add declared-type-aware argument elaboration and shared compiler/oracle partition. Specify and implement safe rules, fixpoint recursion, set semantics, functional conflicts, erasure, and adapter parity with current interface conformance.

## Acceptance Criteria

- [ ] Arguments in `type` columns elaborate declared type terms to SemanticTypeId constants and scoped variables to ID variables.
- [x] Compiler and oracle share one compiler/runtime relation partition.
- [x] Positive safe rules reach a deterministic fixpoint with set semantics.
- [x] Complete compiler keys deduplicate identical rows and refuse distinct projections.
- [x] Compiler facts, rules, and proofs are erased before runtime planning while semantic metadata remains.
- [x] Existing interface conformance runs through an adapter with equivalent diagnostics.

## Tests Run

## Implementation Notes

Authoritative seams: `v6/prolog/compile.pl`, `v6/prolog/conformance/engine.pl`, `v6/prolog/0_generic_expand.pl`, and `v6/prolog/0_program_check.pl`. Introduce one shared partition `partition_compiler_relations(+Decls,-CompilerDecls,-RuntimeDecls)` and evaluator `evaluate_compiler_relations(+CompilerDecls,+SeedRows,-ClosureRows)`. A relation enters this plane when any column has semantic value type `type`; mixing `type` and runtime value domains receives `compiler_relation_mixed_domain(Relation)`. First slice permits positive safe rules and recursion to set fixpoint. Negation receives `compiler_relation_negation_unsupported(Relation)`. Every head variable must occur in a positive body atom. Identical rows deduplicate. For a keyed relation, group by the complete ordered key and refuse distinct non-key projections as `compiler_relation_functional_conflict(Relation, KeyValues)`. Erase declarations, facts, rules, and proof rows before `program_refs`; retain only semantic catalog rows.
