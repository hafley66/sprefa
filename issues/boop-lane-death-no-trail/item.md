---
created: 2026-08-17
updated: 2026-08-17
type: bug
status: fixed
priority: high
labels:
- area:boop
closed: 2026-08-17
---

# boop: a lane can die with no result row, no log, no trace

## Description

2026-08-17 00:0x EDT: lane feature-extract-module-plane-go (flash4, spawned on
the fixed binary, brief pasted whole, fixture written) died with NO result row
on the bus, tmux session gone; the driver's `boop wait --me` sat 540s and
timed out. Same hour: feature-dl6-bytes-target-lowering-2 died with
`supervisor error: write rpc turn/start` (broken pipe into `codex app-server`
stdin, crates/boop/src/channel/jsonrpc.rs:90 via channel/codex.rs:81), while
the same initialize/thread/start/turn/start handshake replays clean by hand.

Diagnosis is impossible after the fact: the supervisor logs only to the pane's
stderr, the pane is gone, `boop db` has no agent_lane rows for either lane,
~/.agent has no per-lane directory. That violates the standing law
"self-diagnosis before execution: the system answers what it was doing from
its own on-disk trail, including after SIGKILL".

## Acceptance Criteria

- [ ] Supervisor tees tracing to ~/.agent/lanes/<lane>/supervise.log (append,
      flushed per line) and the codex/opencode child's stderr to
      ~/.agent/lanes/<lane>/child.stderr.
- [ ] Every exit path of the supervisor writes a result row (rc + reason),
      including panics and SIGHUP/SIGTERM (signal handler or a parent-side
      watcher on the pane).
- [ ] `boop beep lane list` shows a typed reason for a dead lane, never blank.
- [ ] RCA of the two deaths above from the new trail; failure-modes entry.
- [ ] Attribution fix: MAIN-TREE-COMMIT-SUSPECT printed on the wrong lane row
      (extract-module-plane-go blamed for dl6-bytes-target-lowering-2's
      commits cd71912cd, 36f56f008 into the shared sprefa main tree).

## Comments

### 2026-08-17T12:33:32Z · @sprefa-coordinator

LANDED hafley-rs PR #12 (origin/main 982472d): supervise.log + child.stderr under ~/.agent/lanes/<lane>/, panic and SIGHUP/SIGTERM/SIGINT result rows, DEAD=<reason> on lane list, attribution by branch reachability then two-sided author-time window (AMBIGUOUS when overlapping). Installed ~/.cargo/bin/boop 2026-08-17 08:21. Retro RCA of the two original deaths impossible (no trail existed), stated in hafley-rs docs/failure-modes.md entry 1. SIGKILL still yields DEAD=died-before-result with trail files only.
