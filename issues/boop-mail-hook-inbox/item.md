---
created: 2026-08-16
updated: 2026-08-17
type: feature
status: open
priority: high
labels:
- area:boop
---

# boop: coordinator mail via hook inbox, not keystrokes

## Description

User 2026-08-16: "we need a different mail system, interrupting the enter key
and dialog is a bit noisy, how do others solve this". Survey of two local
open-source orchestrators:

| repo | mechanism | receipt |
|---|---|---|
| herdr | keystrokes done atomically: text wrapped in bracketed-paste markers when the pane's paste mode is on, Enter as a separate key; pane idle/working known from agent hooks (Claude Stop/UserPromptSubmit/Notification) reporting to herdr's socket | ~/projects/ext/herdr/src/app/api_helpers.rs:26-60, docs/next/CHANGELOG.md:146 |
| cate | no keystrokes: pi driven by RPC on stdin (`{type:'prompt'}`), claude/codex state via hook stdin->HTTP bridge | ~/projects/cate-local/src/cateAgent/main/piRpcClient.ts:139, src/runtime/capabilities/agentHooks.ts:166 |

Two legs for boop:

1. Short term (BOOPFIX PR): atomic prompt like herdr, bracketed-paste wrap
   plus separate Enter, and never inject while the pane is mid-dialog.
2. This card: claude-kind coordinators get mail through hooks. boop keeps
   its Maildir (~/.agent/mail); a `Stop` hook drains unread hails addressed
   to this session and returns `{"decision":"block","reason":"<hails>"}` so
   the model continues with the mail as input; `UserPromptSubmit` hook
   appends unread hails as context to the user's next prompt. No keystrokes
   into claude panes at all. Lanes (opencode/codex) keep the mailbox poll.
   Optional: an MCP `inbox.list/ack` tool for on-demand reads.

## Acceptance Criteria

- [ ] `boop adopt` for a claude session installs the two hooks (project or
      user settings, idempotent, uninstall path).
- [ ] Hail to a claude coordinator writes mail only; delivery happens at the
      next Stop or UserPromptSubmit; ack recorded.
- [ ] e2e: hail during a long tool call, assert it arrives once, whole, at
      turn end, no pane keystrokes (tmux capture shows none).
- [ ] docs/failure-modes.md entry closes the keystroke-injection era.

## Comments

### 2026-08-17T04:00:56Z · @coordinator

Interim built 2026-08-17 in sprefa: .claude/hooks/boop-inbox.sh (Stop hook returns unread mail as block reason; UserPromptSubmit prints it as context; drained ids in ~/.agent/mail/inbox-drained.<me>) and .claude/hooks/boop-inbox-wait.sh (blocking wait, 540s default, exit 124 + re-run line). Wired in .claude/settings.json. Coordinator route re-registered pane-less so boop hail queues instead of typing. Verified live: hails arrive whole as context, zero keystrokes. Native boop version is card boop-wait-mail-id.
