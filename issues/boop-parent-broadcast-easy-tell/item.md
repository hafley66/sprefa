---
created: 2026-08-16
updated: 2026-08-16
type: feature
status: open
priority: normal
labels:
- area:boop
---

# boop: parent broadcast + least-args tell-parent (me/--me)

## Description

User ask 2026-08-16, verbatim intent: "we need make a parent broadcast and easy
mode boop cli way to tell parent something easy with least args (using me/--me)".

Two legs:

1. **Least-args tell-parent.** Today a child hailing its parent must spell the
   parent's route name: `boop beep hail --body <BODY> <LANE>` (crates/boop, see
   `boop beep hail --help`). A lane/native agent already HAS a parent edge
   (lane create records `--parent`, defaulting "to you then to the one
   registered coordinator"; `boop beep agent register --parent` likewise), and
   `boop whoami` resolves the caller's own identity by rung. Wanted: a spelling
   where the child says only the message and boop resolves the parent from the
   caller's identity — user suggested the `me`/`--me` surface (e.g.
   `boop beep hail --me --body "..."` or a `boop me`-family verb). Least args
   wins; zero route-name knowledge required in agent prompts.

2. **Parent broadcast.** The reverse leg: a parent sends one body to ALL its
   live children (lanes + registered pane-less agents) without enumerating
   them — fan-out resolved from the same parent edges.

## Acceptance Criteria

- [ ] A child with a recorded parent edge can reach its parent with a single
      command carrying only the message body (no route name).
- [ ] A parent can broadcast one message to all live children in one command.
- [ ] Both legs report delivery per target (landed / target dead), same as hail
      does today.
- [ ] `boop --help` doctrine section documents both spellings.

## Comments

Naming collision to resolve at design time: `boop me` currently registers an
interactive Codex tmux pane as a coordinator route; the new surface must not
overload that meaning ambiguously.
