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

# boop: 4 dead lanes across 2 drivers in minutes, two rc=1 signatures

## Description

2026-08-17 ~03:05-03:09, four lane spawns died within a couple of minutes
across two independent drivers, two distinct signatures:

- codex/luna: feature-extract-module-plane-rust DOA, rc=1, supervisor error
  "write rpc turn/start", tmux session never started. Same signature seen
  earlier on feature-shell-v2-terra-wait (receipt: ~/.agent/mail/bus.ndjson).
- opencode/flash4: chore-soopy-public-seams and
  feature-extract-flow-cli-dispatch, rc=1 "stalled: 30s with no harness
  activity", both in the same minute.

Clustering across harnesses and drivers points at the harness/supervisor layer
or machine load (6+ concurrent agents at the time), not the briefs; the
extract driver's attempt-two respawn ran clean (conversation
01a00db1-c222-7ad0-b5d4-bbf903c70c2f, turn start 03:09:08).

RCA open: correlate bus.ndjson timestamps with system load and supervisor
logs; decide whether the 30s stall window is too tight under concurrent
spawns; check whether "write rpc turn/start" is a codex-side race at session
birth.

## Acceptance Criteria

- [x] RCA names the layer for each signature with receipts.
- [x] Spawn path either retries or reports a typed reason the driver can act on.
- [x] docs/failure-modes.md entry (incident, RCA, fail-pre-fix test, rail).

## Comments

### 2026-08-17T03:11:19Z · @coordinator

ESCALATION: the codex signature is NOT transient. Attempt two of feature-extract-module-plane-rust died identically (rc=1, supervisor 'write rpc turn/start', conversation 01a00db1-c222-7ad0-b5d4-bbf903c70c2f, got as far as 'lane turn starting turn_bytes=130' at 03:09:08, brief never opened, zero changed files, HEAD still 4531b4297). 2/2 on codex/luna plus feature-shell-v2-terra-wait earlier = codex-harness lane spawns through boop are deterministically dead right now. Extract driver rerouted the load to a native opus subagent, same worktree/branch/brief, running. Second signature detail from the soopy driver: the 'stalled: 30s with no harness activity' kills fired right after 'tmux send-keys failed socket= argc=5' and 'tui agent window respawned after death', zero bytes written in either worktree - so the prompt likely never landed and the stall detector shot a lane that never got its keys. RCA should split: (a) codex rpc write at turn start, (b) send-keys socket failure feeding the stall detector.

### 2026-08-17T03:59:16Z · @coordinator

Codex signature again 2026-08-16 ~23:58 EDT: lane feature-dl6-bytes-target-lowering (another session's) rc=1 'supervisor error: write rpc turn/start' (bus m-08a9d89e). Now 4/4 codex spawns tonight dead the same way (luna x2, terra x1, this one). Codex-harness lane spawning through boop is deterministically broken; separate from the fixed paste bug (#10).

### 2026-08-19T13:16:10Z · @boop-doa-carcass-lane

RCA closed, no code change: both signatures are one bug and it is already fixed on origin/main (91c9d36). Written up as docs/failure-modes.md entry 8 on hafley-rs branch fix/boop-doa-carcass.

One kill, two exits. ~/.agent/lanes/<lane>/supervise.log for the seven incidents whose lane directory survives shows the identical chain every time, e.g. feature-agent-network-frames/supervise.log:14-22: 'lane turn starting' -> 'lane turn stalled; killing the harness child idle_ms=30453..30559' -> turn_end_reason 'stalled: 30s with no harness activity' retryable=true -> 'lane provider flake; resuming flake_resumes=1' -> 'lane supervisor failed harness=codex error=write rpc turn/start'. The stall window was 30s and a codex reasoning model says nothing until its first tool call, so a healthy child died at ~30s; the flake resume then opened a new turn on the channel the kill had closed and RpcChild::call (crates/boop/src/channel/jsonrpc.rs:111) reported the write into dead stdin. opencode has no rpc turn to re-open, so the same kill exits on the stall string alone. The harness split is exact: every 'write rpc turn/start' row is harness=codex, every 'stalled: 30s' row is harness=opencode.

Codex is not at fault. The rollout for conversation 01a00db1-c222-7ad0-b5d4-bbf903c70c2f ends mid-'reasoning' at 03:09:38.001Z, 29.3s after its session_meta, with no error of its own; seven other rollouts that night end the same way at 27.4-30.0s. Load is not at fault either: 12 spawns in the 15-minute window. 'tmux send-keys failed socket=' and 'tui agent window respawned after death' appear only inside the soopy driver's own bus message (~/.agent/mail/bus.ndjson:1104, :1114); agent_trace_event has zero rows matching either, so no lost keystroke is evidenced.

Already fixed: STALL_LIMIT is 300s at crates/boop/src/supervise.rs:21, sized off a week where 261 in-message gaps ran past 120s. The typed reason the card asks for is 'rpc session closed: <io error>' (channel/jsonrpc.rs:20,99), so a driver reading the string learns the peer was gone. Fail-pre-fix tests already in the tree: supervise.rs:868 a_quiet_opening_gap_is_not_a_stall, channel/jsonrpc.rs:227 a_write_to_a_closed_session_names_the_session, trail.rs:190 a_child_s_stderr_lands_in_the_lane_trail. The scope was wider than the card says: 17 'write rpc turn/start' result rows and 6 'stalled: 30s' rows, 2026-08-16T19:27Z through 2026-08-17T22:28Z, not just the 03:00-03:15 cluster.

### 2026-08-19T13:19:14Z · @boop-doa-carcass-lane

Correction to the note above: the chain in feature-agent-network-frames/supervise.log spans lines 6-15, not 14-22. Independently verified idle_ms values across the surviving lane logs: 30476, 30493, 30516, 30542, 30559 on the 2026-08-17 lanes (the 30s window) against 300078-300699 on feature-boop-tell-parent-pro4 (the 300s window now in force). agent_trace_event rows matching send-keys or respawn: 0.



