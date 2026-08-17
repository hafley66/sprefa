---
created: 2026-08-16
updated: 2026-08-17
type: bug
status: fixed
priority: high
labels:
- area:boop
closed: 2026-08-17
---

# boop: ~/.cargo/bin/boop clobbered by other sessions' builds, install must be from origin/main only

## Description

## Comments

### 2026-08-17T03:53:02Z · @coordinator

2026-08-16: three installs in 10 minutes (23:43 other session no fix, 23:45 BOOPFIX with PR #10, 23:49 other session no fix again). A lane spawned on the 23:49 binary died at 42s and was reported as the fix failing. Fix shape: an install recipe (just install-boop) that refuses unless HEAD is an ancestor of origin/main and the tree is clean, stamps the sha into boop --version, and boop lane create prints its own sha at spawn so a driver can tell which binary ran. Rail: every driver checks boop --version sha before spawn.

### 2026-08-17T04:03:37Z · @coordinator

Install gotcha 2026-08-17: plain cp over the existing ~/.cargo/bin/boop produced 'Killed: 9' on first run (stale macOS code signature). Working recipe: rm, cp, codesign --force --sign - ~/.cargo/bin/boop. Fold into the install recipe.

### 2026-08-17T13:33:37Z · @sprefa-coordinator

LANDED hafley-rs PR #14 (4df5188): just install-boop = fetch, ancestor-of-origin/main + clean-tracked-tree guard, build with BOOP_BUILD_SHA, rm+cp+codesign. boop --version prints 'boop 0.0.2 (<sha>[-dirty])'; lane create's first log line carries boop_build. 321/0 x3. Not yet run on this machine; ~/.cargo/bin/boop is at PR #12 content.


