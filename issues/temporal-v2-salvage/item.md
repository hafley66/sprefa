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
- intent:reconcile
- size:large
- model:large
size: L
lane: typegraph-core
lane_seq: 10
collision: [generic-type-core, compiler-tests]
blocked_by: ['@typegraph-integration-plan']
closed: 2026-08-24
commits:
- hash: f606da9c4
  summary: compiler mixed domains on oracle branch
- hash: 0bedd36a5
  summary: canonical projection oracle
- hash: d2a232b97
  summary: temporal parity oracle
- hash: fdfb80106
  summary: generic substrate integrated to main
---

# Reconcile the temporal v2 worktree with the user-land type graph

## Description

Reconcile `/private/tmp/sprefa-temporal-v2` without losing temporal parity or projection receipts. Separate generic compiler foundations from temporary request builtins.

## Work Sequence

1. Verify branch, base, and dirty state.
2. Apply the integration plan classification to every hunk.
3. Preserve selected mixed compiler-domain and enum-value behavior.
4. Preserve call-form versus legacy temporal runtime parity.
5. Isolate request builtins for later replacement.
6. Keep projection commits separable from temporal commits.
7. Commit with issue trailers and no unrelated formatting.

## Acceptance Criteria

- [x] All 15 dirty or untracked files have keep/move/replace/discard entries.
- [x] Generic compiler-domain tests are independent of temporal syntax.
- [x] Temporal call syntax retains exact plan and runtime timeline parity.
- [x] Projection tests remain independent of implicit parent capture.
- [x] The worktree is clean after commits.
- [x] `@dot-brace-nesting` can land afterward without shared dirty state.

## Tests Run

Focused compiler relations, complete PLUnit, typegen golden, and SQLite temporal execution.

## Implementation Notes

Execution tier: Large, size `L`, label `size:large`. Current Codex owns this dirty-worktree partition directly. Ambiguous hunk ownership stops for user direction.

## Agent Runs

### 2026-08-24T22:16:08Z · @terra-high

Terra-high read-only inventory, 2026-08-24: `/private/tmp/sprefa-temporal-v2` is branch `feature/temporal-relations-v2` at `9e4b46815`, 5 commits behind current main. Index clean; 10 tracked modified plus 5 untracked files; 32 hunks; +712/-23 including untracked lines. Keep C1-C6/P6-P7/T1 for mixed compiler scalar and enum domains. Replace C7-C8/P1-P5/N2 request builtins with user-land normalized storage rows. Preserve projection and temporal implementations as separate parity-oracle commits. `declared_path/3` and `resolve_relation_paths/3` must survive dot-brace parent-capture removal. Full hunk ledger is in `plans/2026-08-24-userland-type-graph.md`.

### 2026-08-24T23:23:52Z · @codex

Preservation split complete on feature/temporal-relations-v2: f606da9c4 generic mixed scalar and enum compiler domains; 0bedd36a5 canonical projection oracle; d2a232b97 temporal call-form parity oracle. Generic/temporal/compiler PLUnit: 53/53. Braced nesting: 30/30. Typegen golden: HOLDS, including temporal DL6, TypeScript, Rust, schema, SQLite retention, SQL-plan parity, and runtime-timeline parity. Worktree clean. Only f606da9c4 is eligible for main under the declared/generated-only scope.
