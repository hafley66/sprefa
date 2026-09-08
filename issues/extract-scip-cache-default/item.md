---
created: 2026-09-08
updated: 2026-09-08
type: chore
reporter: fable
status: open
priority: low
epic: extract-port-closeout
labels:
- pkg:extract
- review-field-test
---

# extract: default --scip-cache under the scratch dir when the root is a clean git worktree

## Description

## Description

The default cache writes `.dl/.state/index.scip` and `.dl/.gitignore` into the reviewed package. In the 2026-09-08 field test the coordinator had to `rm -rf .dl` afterwards and the review lane worked around it with an explicit `--scip-cache`. When the root is a git worktree with a clean status, default the cache to a per-root directory under the OS cache dir keyed by root path hash, and keep the in-root location only when `--scip-cache` names it. See research/2026-09-08-fact-sources-beyond-scip.md §5 row 7.

## Acceptance Criteria

- [ ] a run on a clean worktree leaves `git status` unchanged
- [ ] `--scip-cache` explicit path still honoured
- [ ] help text updated

## Tests Run

- [ ] status-clean receipt

## Implementation Notes

- [ ] reuse probe order unchanged: <ROOT>/index.scip, <ROOT>/.dl/index.scip, then the cache dir
