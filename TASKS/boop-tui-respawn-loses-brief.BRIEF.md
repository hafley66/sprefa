# boop TUI respawn loses the brief

Repo: hafley-rs. Base sha: ff2d67fd2cc9b78fba0bcc5ab2812e1f363a22b2 (origin/main).
FIRST ACTION: `git merge --ff-only ff2d67fd2cc9b78fba0bcc5ab2812e1f363a22b2`. Failure = STOP AND REPORT.

Issue with full mechanism: sprefa repo `issues/boop-tui-respawn-loses-brief/item.md`.

Files you own: `crates/boop/src/channel/tui.rs`, `crates/boop/src/supervise.rs`, and tests
colocated with them. FORBIDDEN: `crates/boop/src/{concatmap.rs,query.rs,main.rs,harness.rs}`
(an open PR owns them), every other crate.

## The defect

- `type_and_submit_or_respawn` (channel/tui.rs:134) retries only the current call's text
  after `reopen_window`. When the dead window is respawned mid-lane, the retried text is a
  steer or probe, never the brief, so a fresh session starts with zero context.
- `reopen_window` (:102) appends `resume_flag` only when `self.conversation` was captured
  before death; nothing guarantees capture happens before the first possible death.
- The stall interrupt can be what kills the TUI (:101), so the watchdog manufactures the
  very death it then mishandles. The claude-channel twin of this was fixed in 7cbef20.

## The fix

1. Capture the conversation id at boot, before the first turn is typed, for every TUI
   harness that has a resume flag. If capture fails, log it loudly at spawn.
2. `reopen_window` with no resumable conversation re-feeds THE BRIEF (supervise owns the
   brief text; pass it into the channel or expose a re-feed callback), never the pending
   nudge. The pending nudge is then sent as a second turn after the brief lands.
3. Before the respawn path runs, verify the window actually died (`window_is_gone` is the
   trigger today; keep it, but the stall-interrupt sender must not treat a live, slow
   window as dead — mirror 7cbef20's last-activity accounting for the TUI channel).

## Receipts

- Fail-first test: fake channel dies mid-lane; assert today's behavior resends only the
  nudge; then the fix resends the brief first. Keep the fail-first output in the test
  header comment.
- Test: a captured conversation id resumes (`resume_flag` present in the respawn command)
  and the brief is NOT re-fed.
- `cargo test -p boop` twice, rc read explicitly, both green.
- `cargo build --release -p boop` rc=0.

Style: tracing only (no eprintln), comments state constraints the code cannot show.
Commit on your branch. Never push. Never touch main.
