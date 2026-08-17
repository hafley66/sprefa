---
created: 2026-08-16
updated: 2026-08-16
type: bug
status: fixed
priority: high
labels:
- area:boop
closed: 2026-08-16
---

# boop-mux: control-mode send-keys %error on multi-line brief, lanes die at 30s

## Description

RCA of the 2026-08-16 23:03-23:13 EDT cluster (5 flash4 lanes, rc=1 "stalled:
30s with no harness activity", zero bytes in every worktree; sibling card
boop-spawn-flake-cluster).

Chain:
1. `ControlClient::command` (crates/boop-mux/src/lib.rs:573-580) joins argv
   with `quote_arg` (lib.rs:630) and writes ONE line to tmux control mode.
2. `send_keys_literal` (lib.rs:314) sends the whole brief as one quoted arg.
3. tmux control mode answers `%error` for a quoted body containing a newline
   followed by `#`, `"`, or `{`. Reproduced 2026-08-16 with `tmux -C attach`:
   `"fn f() { x }"` ok; `"fn f() {\n x\n}"` error; `"# title\nbody"` error;
   `"say \"hi\"\nthere"` error. Every brief has `# heading` lines.
4. Log line: `tmux send-keys failed socket= argc=5` (argc 5 = -t pane -l -- body).
5. Brief never lands, opencode never creates a session row
   (~/.local/share/opencode/opencode.db `session` has no rows for those
   worktrees), `newest_session` returns None, `FIRST_SIGNAL_LIMIT` 30s
   (crates/boop/src/supervise.rs:21) kills the lane. What a driver saw
   "typing" in the pane was the partial brief text, not the agent.

Working spawns earlier the same evening (ping-e2e 21:42, scip-binding 20:40)
had smaller briefs; treat as luck of content, not size.

## Fix shape (pick after reading lib.rs)

- Never put multi-line text on a control-mode command line. Options:
  a. `load-buffer -b <name> <tmpfile>` + `paste-buffer -d -p -t <pane>` (paste
     mode, then a separate `send-keys Enter`).
  b. Exec `tmux send-keys -t <pane> -l -- <body>` as a subprocess (argv, no
     command-line parsing) for bodies containing a newline.
- Add a boop-mux unit test that round-trips a body with `# h\n{\n"q"\n}`
  through the control client into a scratch pane and asserts the pane text.

## Acceptance Criteria

- [ ] Fail-pre-fix test: multi-line brief with `#`, `"`, `{` after newlines
      lands byte-exact in a scratch tmux pane.
- [ ] Spawn of a flash4 lane with TASKS/extract-flow-cli-dispatch.BRIEF.md
      (10.5KB) creates an opencode session and survives past 60s.
- [ ] docs/failure-modes.md entry (incident, RCA, fail-pre-fix test, rail).
- [ ] Rebuilt boop installed to ~/.cargo/bin/boop.

## Comments

### 2026-08-17T03:23:52Z · @coordinator

SECOND LEG, same family: coordinator hails delivered by send-keys into a Claude Code pane arrive CONCATENATED. Multi-line hail bodies trigger bracketed-paste, Enter inside a paste is a newline, so 3-4 hails pile into one user message until an Enter lands outside paste mode (observed 2026-08-16 23:20, bus m-3094f743 + m-6a5cdf9a + m-826aa7b3 arrived as one). Fix shape: after the pasted body, send Enter as a separate key AFTER paste-end, or send body via paste-buffer -p then a delayed Enter; verify against a real claude pane. Also HOPPER's data point: codex spawn turn_bytes=130 means the injected turn is a brief POINTER not the body; BOOPFIX must confirm what exact text hit send-keys on the killed opencode lanes (goal string? pointer line?) and match it to the trigger, so the RCA cites the real bytes.

### 2026-08-17T03:45:01Z · @boopfix

FIXED, but not by the RCA on this card. Measured: send_keys_literal/send_text never went through control mode (git log -S'command(&["send-keys"' is empty, always); the control-mode %error is a real tmux parser fact off the brief path. Real cause: send-keys -l types a body rune by rune. 10540-byte brief into a live opencode TUI: still ingesting at 70s, first session row ~110s after Enter, vs FIRST_SIGNAL_LIMIT 30s. Same root cause for the hail-concatenation leg (Enter typed inside a TUI's paste-grouping window is a newline). Fix: hafley-rs PR #10, paste_body = load-buffer + paste-buffer -d -p, submit key after SUBMIT_GAP 400ms. Fail-pre-fix tests a_multiline_body_reaches_a_pasting_pane_bracketed_and_byte_exact + a_brief_sized_body_arrives_whole (RED: 10401 of 10413 bytes in 10.1s), post-fix 11 passed; cargo test -p boop 281 passed 0 failed. Live: opencode session row 3s after Enter with the same brief; 3 multi-line hails into a Claude Code pane = 3 separate user messages. Ledger sprefa PR #334 (failure-modes 52). Binary installed ~/.cargo/bin/boop, boop 0.0.2, mtime Aug 16 23:42. NOT DONE: the boop beep lane create end-to-end spawn was permission-denied in my session, so acceptance criterion 2 is unverified.

