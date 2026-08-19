---
created: 2026-08-16
updated: 2026-08-19
type: feature
status: done
priority: high
labels:
- area:boop
related: ['@boop-yield-to-parent']
closed: 2026-08-19
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

- [x] A child with a recorded parent edge can reach its parent with a single
      command carrying only the message body (no route name).
- [x] A parent can broadcast one message to all live children in one command.
- [x] Both legs report delivery per target (landed / target dead), same as hail
      does today.
- [x] `boop --help` doctrine section documents both spellings.

## Comments

Naming collision to resolve at design time: `boop me` currently registers an
interactive Codex tmux pane as a coordinator route; the new surface must not
overload that meaning ambiguously.

## Decisions

### 2026-08-19T00:01:26Z · @codex

2026-08-18 observed receipt: a spawned lane already has caller identity plus parent edge, yet completion required spelling codex-147 in 'boop beep hail'. Required minimal surface: 'boop tell-parent --kind completion --body ...'; resolve caller through whoami/harness trait, then parent from registered edge, and return the mail message ID.

### 2026-08-19T13:11:49Z · @opus-tell-parent

Both legs landed in hafley-rs PR #33 as boop tell-parent [--kind completion|yield|note] [--body TEXT] and boop tell-children --body TEXT. No me/--me spelling and no boop me subcommand, so the boop me collision named in the comments does not arise. tell-children prints landed <name> <id> (<how>) or dead <name> per target and writes no row for a dead child. Two doctrine lines in boop --help.

