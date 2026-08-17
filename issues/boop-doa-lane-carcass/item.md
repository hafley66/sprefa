---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: open
priority: normal
labels:
- area:boop
---

# boop: DOA lane leaves carcass that blocks respawn, lane delete cannot clean it

## Description

Observed 2026-08-17 ~03:05 by the extract driver after a dead-on-arrival codex
spawn (feature-extract-module-plane-rust, rc=1 "write rpc turn/start", tmux
session never started):

- `boop beep lane create` for the same branch refuses with
  "worktree path already exists".
- `boop beep lane delete <lane>` errors "no registry route" because the
  on-exit hook already tore the route down.
- Only manual `git worktree remove --force` + `git branch -D` unblocks the
  respawn.

A DOA spawn is exactly the case where retrying the same lane name is the
obvious next move; the carcass making that a two-command manual dig is a rail
gap. Wanted: either lane create gains an idempotent reclaim of a dead lane's
worktree/branch, or lane delete works on a carcass with no live route.

## Acceptance Criteria

- [ ] After a DOA spawn, one boop command (create with reclaim, or delete)
      returns the lane name to spawnable, no manual git surgery.
- [ ] Covered by a test that kills a spawn pre-turn and respawns the same name.
- [ ] docs/failure-modes.md entry lands with the fix (incident, RCA,
      fail-pre-fix test, rail).

## Comments

### 2026-08-17T03:11:20Z · @coordinator

Confirmed by a second driver same night: soopy driver's two stall-killed lanes also answered 'no registry route' on boop beep lane delete and needed manual git worktree remove before respawn. Two drivers, four carcasses, same manual dig.
