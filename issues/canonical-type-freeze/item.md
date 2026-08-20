---
created: 2026-08-20
updated: 2026-08-20
type: feature
assignee: codex
status: done
priority: high
epic: relational-type-schema
labels:
- area:dl6
- intent:type-system
closed: 2026-08-20
---

# Freeze canonical type rows after minting

## Description

Plan: `v6/plans/2026-08-20-canonical-type-row-pipeline.md`.

Move the semantic type-row freeze after generic, anonymous, option, enum, key,
and annotation minting. Establish one complete semantic authority before
compiler queries and physical lowering.

## Acceptance Criteria

- [x] Every declared and generated type has one canonical declaration row.
- [x] Every declared and generated field has one canonical member row.
- [x] Wrapper and generic applications retain ordered semantic arguments.
- [x] Every member application resolves its constructor and complete arguments.
- [x] Duplicate semantic identities produce a named compiler refusal.
- [x] Compiler and oracle produce equal canonical row sets.

## Tests Run

- `swipl -q -l v6/prolog/compile/test/type_relation_ir.test.pl -g run_tests -g halt` (54 passed)
- `swipl -q -l v6/prolog/compile/test/compiler_relations.test.pl -g run_tests -g halt` (15 passed)
- `swipl -q -l v6/prolog/compile/test/anonymous_type_syntax.test.pl -g run_tests -g halt` (20 passed)
- `swipl -q -l v6/prolog/compile/test/anonymous_product_values.test.pl -g run_tests -g halt` (7 passed)
- `cd v6 && swipl -q -l prolog/compile.pl -g "compile:compile_dl6('dl/fixtures/0_typespec_basic_probe.dl6','/private/tmp/canonical-typespec-probe.ts')" -g halt` (passed)
- `cd v6 && swipl -q -l prolog/compile.pl -g "compile:compile_dl6('dl/fixtures/type-annotation-ci.dl6','/private/tmp/canonical-type-annotation-ci.ts')" -g halt` (passed)
- `cd v6 && just plunit` (931 tests, 23 failures, exit 1)
- `swipl -q -l v6/prolog/compile/test/plunit_tests.pl -g "run_tests(relation_id_access)" -g halt` (7 passed after adding canonical `id(T)` application rows)
- A focused seven-unit comparison against held commit `e3702cd40` found ten
  matching failures and two correction-introduced `relation_id_access`
  failures. Adding `id(T)` to the canonical wrapper graph fixed both.

## Implementation Notes

- Canonical rows are authoritative during compiler evaluation and final
  completed expansion. Mutable declaration carriers are producers only.
- Canonical member rows carry field identity, position, name, and type.
  Canonical member-role rows carry self, key, return, and anonymous-owner
  roles. Carrier-derived roles remain only as the no-canonical-row bootstrap.
- The issue remains in testing because the single full run had 23 failures.
  The two failures introduced by this correction passed after the wrapper fix;
  the full suite was not rerun.

## Comments

### 2026-08-20T15:18:16Z · @terra

Canonical freeze implementation: generated type/member rows merge into the existing identity graph before compiler evaluation and after runtime expansion; focused compiler suites and the TypeSpec probe passed.

### 2026-08-20T15:28:43Z · @terra

One full `just plunit` run was recorded before the latest focused corrections. Its captured output was truncated before final summary, so the issue remains in testing pending the root integration run.

### 2026-08-20T17:32:13Z · @codex

Rebased integration CI: just --justfile v6/justfile plunit, 997 passed, 0 failed.

