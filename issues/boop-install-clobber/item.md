---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: open
priority: high
labels:
- area:boop
---

# boop: ~/.cargo/bin/boop clobbered by other sessions' builds, install must be from origin/main only

## Description

## Comments

### 2026-08-17T03:53:02Z · @coordinator

2026-08-16: three installs in 10 minutes (23:43 other session no fix, 23:45 BOOPFIX with PR #10, 23:49 other session no fix again). A lane spawned on the 23:49 binary died at 42s and was reported as the fix failing. Fix shape: an install recipe (just install-boop) that refuses unless HEAD is an ancestor of origin/main and the tree is clean, stamps the sha into boop --version, and boop lane create prints its own sha at spawn so a driver can tell which binary ran. Rail: every driver checks boop --version sha before spawn.
