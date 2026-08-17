---
created: 2026-08-16
updated: 2026-08-16
type: improvement
reporter: chris
status: open
priority: normal
epic: soopy-full-wiring
---

# dep-crawl manifest leg reads at the pinned rev

## Description

The crawl's git leg is rev-pinned (dep_resolve.rs:476) but go.mod/package.json reads are raw worktree fs (dep_resolve.rs:150,410,418). Route manifests through the same CheckoutTrees::read_each. Candidate 9.
