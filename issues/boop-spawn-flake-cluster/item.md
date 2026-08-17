---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: open
priority: normal
labels:
- area:boop
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

- [ ] RCA names the layer for each signature with receipts.
- [ ] Spawn path either retries or reports a typed reason the driver can act on.
- [ ] docs/failure-modes.md entry (incident, RCA, fail-pre-fix test, rail).

## Comments

### 2026-08-17T03:11:19Z · @coordinator

ESCALATION: the codex signature is NOT transient. Attempt two of feature-extract-module-plane-rust died identically (rc=1, supervisor 'write rpc turn/start', conversation 01a00db1-c222-7ad0-b5d4-bbf903c70c2f, got as far as 'lane turn starting turn_bytes=130' at 03:09:08, brief never opened, zero changed files, HEAD still 4531b4297). 2/2 on codex/luna plus feature-shell-v2-terra-wait earlier = codex-harness lane spawns through boop are deterministically dead right now. Extract driver rerouted the load to a native opus subagent, same worktree/branch/brief, running. Second signature detail from the soopy driver: the 'stalled: 30s with no harness activity' kills fired right after 'tmux send-keys failed socket= argc=5' and 'tui agent window respawned after death', zero bytes written in either worktree - so the prompt likely never landed and the stall detector shot a lane that never got its keys. RCA should split: (a) codex rpc write at turn start, (b) send-keys socket failure feeding the stall detector.

### 2026-08-17T03:59:16Z · @coordinator

Codex signature again 2026-08-16 ~23:58 EDT: lane feature-dl6-bytes-target-lowering (another session's) rc=1 'supervisor error: write rpc turn/start' (bus m-08a9d89e). Now 4/4 codex spawns tonight dead the same way (luna x2, terra x1, this one). Codex-harness lane spawning through boop is deterministically broken; separate from the fixed paste bug (#10).

