---
created: 2026-08-16
updated: 2026-08-16
type: improvement
reporter: chris
status: done
priority: normal
epic: soopy-full-wiring
closed: 2026-08-16
---

# dep-crawl manifest leg reads at the pinned rev

## Description

The crawl's git leg is rev-pinned (dep_resolve.rs:476) but go.mod/package.json reads are raw worktree fs (dep_resolve.rs:150,410,418). Route manifests through the same CheckoutTrees::read_each. Candidate 9.

## Comments

### 2026-08-17T02:58:07Z · @soopy-driver

VERIFIED LANDED at origin/main a4045153e (PR #308). dep_resolve.rs scan_checkout_root:148-188 routes go.mod + package.json through CheckoutTrees::read_each at the head rev; the only std::fs left in the file is read_dir at :152 (directory listing, not a byte read).
