---
created: 2026-08-28
updated: 2026-08-29
type: task
assignee: glm53f
status: needs-info
priority: normal
epic: dl7-minimal-kernel
labels: [dl7, model-glm53f]
size: M
lane: dl7-kernel
lane_seq: 2
collision: [v7-kernel, v7-prelude]
blocked_by: ['@dl7-luna-review', '@dl7-relational-cons', '@dl7-stratified-negation', '@dl7-count-aggregate', '@dl7-ordered-index', '@dl7-edge-snapshot-ruling']
---

# Add userland Pick and Exclude goals

## Description

## Description

After the kernel review passes, add Pick and Exclude as `.dl7` standard-library
rules. Reuse the one oracle fixture and expected term.

## Acceptance Criteria

- [ ] Operator names occur only in prelude, fixture, and documentation.
- [ ] Pick uses a positive symbol-membership join.
- [ ] Exclude uses a completed lower-stratum anti-join.
- [ ] Both preserve relative order and assign dense output indices.
- [ ] The existing single oracle expands without adding a test.

## Test Run

Run the single SWI command once.

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
