---
created: 2026-08-17
updated: 2026-08-17
type: task
status: open
priority: normal
related: ['@boop-parent-broadcast-easy-tell']
labels:
- area:boop
---

# boop: one verb for a child to tell its parent chat it yields (done for current task)

## Description

## Ask (2026-08-17)
One boop verb a child (lane, native agent, or resident chat) runs when it is
done with its current task, that tells its parent chat "I yield" with the
fewest possible args: `boop yield [--body "<one line>"]` (spelling open;
`boop beep yield` acceptable if the beep namespace is kept).

## Behavior
- Resolves the caller (`boop whoami` rung) and the caller's parent edge (lane
  `--parent`, agent register `--parent`, else the one registered coordinator).
- Writes a `kind=yield` mail row addressed to the parent, body optional,
  default `yield <lane-id> rc=0 branch=<branch> head=<sha>`.
- Delivery through the same pane injection / native hook drain the
  completion hail uses (hafley-rs PR #6, #15), so the parent sees it mid-turn
  or idle.
- Exit 0 once the row is written; a missing parent edge is a named error, not
  a silent no-op.
- Does NOT kill the child, does NOT close the lane; it is a turn boundary,
  the lane may be hailed again.

## Relation to @boop-parent-broadcast-easy-tell
That card's leg 1 (least-args tell-parent) is the general "say X to parent";
this card is the one-word specialisation for the "done for now" case, and can
be built as `tell-parent --kind yield` under the hood. Land whichever first;
the other reuses it.

## Receipt
Spawn a lane on a throwaway tmux socket, have it run `boop yield`, assert the
parent route's inbox has one `kind=yield` row and `boop inbox drain` prints it.
