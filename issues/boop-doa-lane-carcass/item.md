---
created: 2026-08-16
updated: 2026-08-19
type: bug
status: fixed
priority: normal
labels:
- area:boop
closed: 2026-08-19
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

- [x] After a DOA spawn, one boop command (create with reclaim, or delete)
      returns the lane name to spawnable, no manual git surgery.
- [x] Covered by a test that kills a spawn pre-turn and respawns the same name.
- [x] docs/failure-modes.md entry lands with the fix (incident, RCA,
      fail-pre-fix test, rail).

## Comments

### 2026-08-17T03:11:20Z · @coordinator

Confirmed by a second driver same night: soopy driver's two stall-killed lanes also answered 'no registry route' on boop beep lane delete and needed manual git worktree remove before respawn. Two drivers, four carcasses, same manual dig.

### 2026-08-19T13:16:10Z · @boop-doa-carcass-lane

Fixed on hafley-rs branch fix/boop-doa-carcass, PR open, not merged. Chris decided the CLI 2026-08-19: 'boop beep lane create --reclaim' removes the dead lane's worktree and branch then spawns; 'boop beep lane delete <lane>' works on a carcass with no registry route and prints what it removed; no other new verbs.

Where it lives: worktree.rs reclaim_carcass does the git surgery and holds both stops (a worktree with uncommitted changes, a branch carrying commits no other ref has, printed back to the caller); lane.rs find_carcass matches a lane id against the repo's own 'git worktree list --porcelain' by branch slug, delete_carcass and reclaim_for_spawn add the liveness stop (a live tmux target refuses; a dead target has no pane pid left, so one question answers both); main.rs carries only the flag and two pass-through hunks. The plain bail now names both escapes.

Tests: crates/boop/tests/lane_carcass.rs spawns a real DOA lane (harness absent from a throwaway PATH, throwaway tmux socket, HOME and BOOP_DB in temp) and waits for the epilogue to drop the route, then asserts the plain respawn bails naming --reclaim, the flagged respawn rebuilds the worktree, lane delete clears the carcass and names both removals, and a dirty worktree refuses. Pre-fix receipt: the installed binary answers 'unexpected argument --reclaim found'. 3 tests, 6.44s. Whole battery cargo test -p boop --no-fail-fast 428 passed, 0 failed, 36.7s wall.

Ledger: docs/failure-modes.md entry 7.

