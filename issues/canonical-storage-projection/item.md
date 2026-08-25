---
created: 2026-08-20
updated: 2026-08-24
type: feature
assignee: codex
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
blocked_by: ['@canonical-type-freeze']
commits:
- hash: d6e5ffecc
  summary: derive TypeId/MemberId-keyed storage rows and reconstruct catalog plans
- hash: e83782476
  summary: canonicalize generated member names and primitive storage types
closed: 2026-08-24
closed_by: codex
---

# Derive physical storage from canonical type rows

## Description

Plan: `v6/plans/2026-08-20-canonical-type-row-pipeline.md`.

Replace post-freeze semantic reads of declaration carriers with
TypeId/MemberId-keyed storage projections, then serialize the catalog from
semantic and physical rows.

## Acceptance Criteria

- [x] Storage relations reference canonical TypeId values.
- [x] Storage columns reference canonical MemberId values.
- [x] Physical rows contain target facts without copied semantic fields.
- [x] Catalog rows are serialized from semantic and physical rows.
- [x] TS, Rust, and SQLite executable CI passes.

## Tests Run

- Focused storage and catalog consumers: 119/119 passed.
- Full `v6/prolog/compile/test/plunit_tests.pl`: 1092/1092 passed in 16 seconds.
- The full suite includes Rust type compilation, TypeScript and Rust plan
  emission, and SQLite DDL execution coverage.
- `typegen_golden.sh` product/sum stages passed for Prolog, TSV2, and Rust. Its
  local HTTP-server stage was outside this issue and could not bind a sandbox
  port.

## Implementation Notes

- `storage_relation/3` is keyed by canonical `TypeId`.
- `storage_column/2` is keyed by canonical `MemberId`.
- `storage_key/2` links canonical relation and member identities.
- Catalog serialization reconstructs compatibility `rel/5` plans from
  semantic and physical rows.
- Undeclared IDB relations retain their existing `rel/5` plans and receive no
  inferred canonical identity.

## Agent Runs

### 2026-08-24T22:16:09Z · @terra-high

Terra-high read-only review, 2026-08-24: stale worktree base `a5929de0a` is 429 commits behind current main. Current freeze occurs in `0_generic_expand/0b_expansion_pipeline.pl`; `program_plan/3` computes RelPlans later. Probe gap: inferred `derived/1` receives `rel/5` but no canonical declaration/member rows. Reimplementation must complete canonical runtime relation shapes before final semantic freeze, use module-aware IDs, keep storage projection pure, bridge MemberId to dense catalog ColumnId, and retain `rel/5` compatibility during migration. Current direct runtime target reads: `lower.pl` 79, `emit_ts.pl` 23, `emit_rust.pl` 12. Focused current-main `type_relation_ir` tests passed 56/56.

## Decisions

### 2026-08-24T23:14:04Z · @codex

Storage projection covers relations and members that already have canonical declared or compiler-generated identities. An undeclared IDB keeps its executable `rel/5` plan and receives no manufactured `TypeId` or `MemberId`. The projector skips plans without canonical owners and never inserts semantic rows.

## Resolution

### 2026-08-25T01:47:42Z · @codex

Implemented canonical storage_relation(TypeId,...), storage_column(MemberId,...), and storage_key(TypeId,MemberId) rows; catalog serialization reconstructs relplans from semantic and physical rows while undeclared IDBs retain rel/5 compatibility. Verification: focused compiler and consumer matrix 119/119; full Prolog suite 1092/1092 in 16 seconds, including executable TypeScript/Rust plan generation and SQLite DDL execution.
