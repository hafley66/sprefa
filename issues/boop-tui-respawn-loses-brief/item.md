---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: open
priority: high
labels:
- area:boop,size:med
---

# boop TUI respawn loses the brief: dead opencode window respawns with only the current nudge text

## Description

## Comments

### 2026-08-16T17:39:42Z · @fable

MECHANISM (hafley-rs crates/boop/src/channel/tui.rs, verified 2026-08-16): type_and_submit_or_respawn (:134) retries only the CURRENT call's text after reopen_window. reopen_window (:102) appends resume_flag only when self.conversation was captured before death. Chain: stall interrupt kills the opencode TUI (:101 comment says a TUI may exit on the interrupt) -> next injection is a steer/probe, not the brief -> respawned session has no conversation and no brief -> lane wanders or no-ops -> rc=0 with an empty worktree. Observed 2026-08-16: 4 flash4 lanes 0-for-4 (three empty worktrees, one delivered-uncommitted), one pro4 lane 'session gone at ~4 min'; a fifth flash4 lane (chore-cpg-spec-research) ran fine when nothing stalled, proving the model was never the variable. Fix direction: (a) capture the opencode conversation id at boot, before any turn; (b) reopen_window without a resumable conversation re-feeds THE BRIEF, never the pending nudge; (c) the stall interrupt verifies the window actually died before respawn accounting, mirroring the claude-lane watchdog fix 7cbef20. Receipts: a fake-channel test where death mid-lane leads to a respawn that resends the brief; a test that a resumable conversation resumes instead.
