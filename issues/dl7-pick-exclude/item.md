---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: glm53f
status: done
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f]
size: M
lane: dl7-kernel
lane_seq: 2
collision: [v7-kernel, v7-prelude]
blocked_by: ['@dl7-luna-review', '@dl7-relational-cons', '@dl7-stratified-negation', '@dl7-count-aggregate', '@dl7-ordered-index', '@dl7-edge-snapshot-ruling']
closed: 2026-08-29
closed_by: codex
commits:
- hash: d2d7410c0
  summary: Complete chained type operator rounds
---

# Add userland Pick and Exclude goals

## Description

## Description

After the kernel review passes, add Pick and Exclude as `.dl7` standard-library
rules. Reuse the one oracle fixture and expected term.

## Acceptance Criteria

- [x] Operator names occur only in prelude, fixture, and documentation.
- [x] Pick uses a positive symbol-membership join.
- [x] Exclude uses a completed lower-stratum anti-join.
- [x] Both preserve relative order and assign dense output indices.
- [x] The existing single oracle expands without adding a test.

## Tests Run

- [x] Consolidated V7 SWI suite: 14 of 14 tests passed.
- [x] V7 Tree-sitter corpus: 1 of 1 parses passed.

## Implementation Notes

Compiler rounds expose the previous complete `':'/4` set through
`edge_snapshot/4`. Application demand crosses rounds through
`intern_snapshot/3`. `Exclude` materializes `excluded_name/3` before its
completed lower-stratum anti-join.

## Stop condition

Hail the parent if dense ranking requires an unplanned aggregate primitive.

## Blocker evidence

The current checked representation is positive-only: rule bodies contain
`call/2`, `depends_rows/2` emits only `depends(_, _, positive)`, and
`strata_rows/2` emits only `stratum(_, 0)`.  It has no negative-goal syntax
lowering, negative dependency, or completed lower-stratum evaluation for the
Exclude anti-join.

`kernel(cons)/3` is the only list-shaped relation.  Its evaluator clause
requires ground head and tail and constructs the list result, so a userland
rule cannot traverse a supplied symbol list for Pick membership.  The only
dense-index operation validates source bind indices; no predecessor count,
dense-rank relation, arithmetic, comparison, or aggregate representation is
checked.

Detailed evidence: `v7/tasks/results/8_PICK_EXCLUDE.md`.

The extension review adds three required inputs: ordered strict-predecessor
rows or comparison, explicit zero-rank handling, and closure functional-key
validation for derived `':'/4` rows. Exact rule shapes and the corrected DAG are
in `v7/design/3_DATALOG_EXTENSIONS.REVIEW.md`.

## Resolution

### 2026-08-29T23:39:08Z · @codex

Pick and Exclude are prelude Datalog rules. Exclude materializes excluded_name/3 before its lower-stratum anti-join; both operators preserve relative order and assign dense indices in the existing oracle.
