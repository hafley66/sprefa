---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: fixed
priority: normal
labels:
- area:boop
closed: 2026-08-16
closed_by: fable
---

# boop claude harness: lane exits rc=1 after completing its deliverable, sometimes without committing

## Description

Three claude-harness lanes on 2026-08-15/16 (opus model, spawned via lane create --harness claude): chore-generics-wrapper-inspection (committed doc 1, died before doc 2, rc=1), chore-fuzz-grammar-threedoor-plan (both docs committed, work complete, rc=1), feature-dep-resolve-recursion (full deliverable written, tests passing, NOTHING committed, rc=1). Pattern: the deliverable exists but exit_code=1 rides the lane result, and commit behavior is inconsistent. Wanted: RCA of what exit code the claude CLI returns at session end under boop's spawn wrapper, and whether the wrapper conflates a nonzero final-command rc with lane failure. Until then coordinators treat claude-lane rc as meaningless (same doctrine as opencode rc=0).

## Resolution

### 2026-08-16T05:22:31Z · @fable

Landed hafley-rs PR #3: rc never came from the CLI; stall watchdog used the 30s first-signal bound because ClaudeChannel never overrode last_activity_ms. Channel now stamps child output activity.
