---
created: 2026-08-18
updated: 2026-08-18
type: task
assignee: luna
status: done
priority: normal
epic: relational-type-schema
labels:
- area:dl6
- intent:ci
blocked_by: ['@interface-bound-transport', '@compiler-type-relations', '@key-wrapper-normalization', '@anonymous-product-values', '@rust-associated-outputs', '@anonymous-sum-values']
lane: ci
lane_seq: 0
collision: [type-emitters, ci-scripts]
closed: 2026-08-18
commits:
- hash: 139d834f5
  summary: 'ci: wire relational type cross-target battery'
- hash: 5d8fe7176
  summary: 'ci: wire relational type cross-target battery (final)'
---

# Wire relational type cross-target CI

## Description

Add real DL6 source fixtures through parser, expansion, catalog and typegen; direct/DL6 parity; tsc compilation; Rust temporary-crate compilation; JSON Schema validation; runtime product tests; selected sum tests; and include typegen in CI.

## Acceptance Criteria

- [x] Real `.dl6` fixtures cross parser, expansion, catalog, and typegen export.
- [x] Direct Prolog and DL6 type renderers agree on complete semantic rows.
- [x] Generated TypeScript passes `tsc --noEmit`.
- [x] Generated Rust passes a temporary-crate build and focused execution tests.
- [x] Generated JSON Schema passes metaschema and instance validation.
- [x] Product runtime tests and the selected sum runtime tests pass in both engines.
- [x] Typegen runs in the repository CI battery.

## Tests Run

## Implementation Notes

Use compiler-authored fixtures under `v6/dl/fixtures/`, generated rows/artifacts under `v6/prolog/compile/test/typegen_golden/`, TS compilation through the existing `v6/tsv2` toolchain, and a temporary Rust crate with `serde` under the test script's temporary directory. Extend `v6/prolog/compile/test/typegen_golden.sh` to compile generated TS/Rust and validate JSON Schema instances, then add that script to the repository `green` CI recipe. Runtime product/sum fixtures must execute through both TSV2 and `v6/sprefa-engine-rs`. Do not treat hand-authored JSONL renderer fixtures as compiler end-to-end evidence.

## Agent Runs

### 2026-08-19T02:13:24Z · @codex-147

typegen_golden.sh HOLDS: 4 term-fixture TS/Rust parity pairs; 2 authored .dl6 fixtures (anonymous-type-syntax, rust-associated-outputs) pass parser/use expansion/catalog/typegen export, direct Prolog/DL6 TS/Rust parity, tsc --noEmit, temporary serde Cargo tests, and generated-schema metaschema plus representative instance validation. Runtime: anonymous_product_values + anonymous_sum_values Prolog 10/10; TSV2 enumPlane 3/3; Rust enum_plane library 8/8. CI: v6 just green and green-parallel now include typegen-golden. Syntax checks: bash -n, node --check, git diff --check.
