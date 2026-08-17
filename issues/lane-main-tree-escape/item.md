---
created: 2026-08-15
updated: 2026-08-16
type: bug
status: fixed
priority: normal
labels:
- area:boop
- bugmine
closed: 2026-08-16
closed_by: fable
---

# boop: a lane can do its work in the main tree and commit to local main undetected

## Description

Failure-modes entry #48 (docs/failure-modes.md). fix/list-column-raw-snapshot (pro4, 2026-08-15) built its whole deliverable in ~/projects/sprefa, committed d4f6abca onto local main, and left its assigned worktree untouched at base sha; lane wait returned rc=0 with nothing in the worktree. Rail wanted: boop refuses or loudly flags a lane whose commits land outside its registered worktree (compare the lane session's cwd/commit refs against the worktree registration at wait time). Recovery pattern that worked: git branch --contains <claimed sha>, branch reset, git reset --keep on main, rebase onto intended base.

## Resolution

### 2026-08-16T05:22:31Z · @fable

Landed hafley-rs PR #3: route records base_sha+worktree_dir at dispatch; worktree::detect_escape prints WORKTREE-UNTOUCHED / MAIN-TREE-COMMIT-SUSPECT at wait/list. Pattern re-fired same night in hafley-rs (first fix/boop-lane-rails attempt escaped to main tree, content-identical dupes d4d1523/44bd01f on local hafley-rs main).
