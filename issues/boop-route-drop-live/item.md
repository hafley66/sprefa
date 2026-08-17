---
created: 2026-08-16
updated: 2026-08-16
type: bug
reporter: chris
status: fixed
priority: high
labels:
- area:boop
closed: 2026-08-16
---

# boop deletes lane route on rc=1 while the opencode process is still alive

## Description

Observed 2026-08-16 during extract-df-aux-fields-lits: flash4 lane burned five flake resumes; boop posted rc=1 and deleted the lane route while the opencode TUI process kept running. Hails then had no delivery path and the driver typed into the tmux pane by hand. Expected: the route lives as long as the process, or the process is killed with the route. Sibling of the fixed boop-tui-respawn-loses-brief (hafley-rs PR #5); likely in the supervise flake-resume exhaustion path.

## Comments

### 2026-08-17T00:53:07Z · @fable

Two more occurrences 2026-08-17: docs-joern-practice-problems and feature-scip-binding-text-slice both had no registry route by lane completion (lane delete errored no-route) while their tmux sessions and lane wait rows worked fine. Route drop is now the normal end state for every completed lane tonight, not an outlier.

### 2026-08-17T01:30:54Z · @fable

RCA CORRECTION + full ping-path autopsy (coordinator read boop source tonight). (1) Route deletion at lane COMPLETION is BY DESIGN: lane create's on_exit epilogue runs 'boop hail --kind result ... ; boop beep lane delete <lane> --route-only' (hafley-rs boop/src/main.rs:2086-2095). My earlier note calling completed-lane route absence a defect was a misdiagnosis. The card's ORIGINAL defect stands: the epilogue also fires when supervise gives up (rc=1 flake exhaustion) while the opencode TUI process still lives, killing steering. (2) WHY THE COORDINATOR NEVER GETS PINGED: the epilogue does hail the parent, but the SessionStart adopt hook writes the coordinator route with kind='lane' hardcoded (run_adopt, main.rs:2252) and run_hail short-circuits kind=='lane' with 'lane supervisor delivers it' (main.rs:1689-1695). A real lane has a supervisor polling the mailbox; an adopted Claude session has none, so every result row addressed to sprefa-coordinator queues in bus.ndjson forever with to_timestamp null. Fix candidate: adopt writes kind='coordinator'; run_hail's coordinator branch already delivers via claude-harness send_keys_literal (text + Enter into the pane, boop-mux/src/lib.rs:297) gated on target_alive, which handles pane targets correctly (lib.rs:188). (3) Cosmetic: lane list shows the coordinator 'dead' because lane_state uses LiveSessions::has(session-name) against the pane target 'sprefa:0.0' (main.rs:4237) instead of target_alive.

## Resolution

### 2026-08-17T01:43:11Z · @issuectl

Fixed in hafley-rs PR #6 (merged e248a41). Three fixes: (1) boop adopt writes kind=coordinator so run_hail delivers by pane injection instead of the lane-supervisor branch that queued coordinator mail forever; (2) lane_state resolves pane targets to sessions (coordinator listed dead); (3) TuiChannel::close escalates C-c x2 to kill_window so a give-up cannot leave a live opencode TUI after the route is deleted. Receipts: tests/coordinator_ping.rs e2e over real tmux (FAIL-PRE-FIX header), full boop battery green 2x, and a LIVE weak-model proof: q38 lane chore-ping-e2e completed and its ping arrived typed into the coordinator pane with no lane wait armed; the lane tmux session was fully gone afterward (close escalation confirmed).
