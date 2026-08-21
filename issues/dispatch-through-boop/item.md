---
created: 2026-08-21
updated: 2026-08-21
type: chore
reporter: chris
assignee: chris
status: open
priority: normal
epic: cheap-fast-analysis
labels: [dispatch]
---

# Dispatch through boop lanes with a PR per arc; timeouts on every command

## Description

Subagents spawn via boop beep lane create (claude harness, opus), open a GitHub PR with receipts, never a bare tmux or Agent-tool worktree. Every command in a brief wraps timeout. The PR stream is the data ghcacher proves itself on.
