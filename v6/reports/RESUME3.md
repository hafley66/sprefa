# LANE boop — RESUME. Your previous run died with tmux; the worktree survived.

Read, in this order: STANDING.md (dependency rule is revoked, pick your own),
PASS2.md, PASS2-ADDENDUM.md. All three still bind except where STANDING.md
overrides.

## Where you actually are (coordinator-verified, do not re-derive)

- Pass 1 IS COMMITTED: `b3428e68` "boop: PASS — pass 1 claude transcript tailer
  scaffold". The PASS2.md section-0 claim that nothing is committed is STALE.
- Your pass-2 work sits UNCOMMITTED in the worktree: new `v6/boop/src/bus.rs`,
  new `v6/boop/src/ident.rs`, modified `main.rs` (+~1000 lines), modified
  `harness/claude.rs`, Cargo.toml/lock. 1457 insertions total.
- First action: `git status --short && git diff --stat`, then
  `cd v6/boop && cargo build && cargo test && cargo clippy -- -D warnings`.
  Fix what is red, then continue pass 2 from where the diff leaves off.

## North star, user's words this session

"with boop i hope we can make subagents literally turnkey."

Turnkey = one brief in, done-event out. Today no tmux lane of any harness emits
a completion event; every coordinator hand-arms a watch. boop's tailer is that
missing signal. Pass 2's bus 1-1 scope should keep this front and center: spawn
+ register + tail + completion detection is ONE flow through boop.

## Deliverables

- Commit on `lane/boop` when `cargo build`, `cargo test`,
  `cargo clippy -- -D warnings` are all green.
- REPORT.md at worktree root: what pass 2 now covers, what remains.
- Done-report, ALWAYS your last action, success or not:
  `bus hail --to fable-main --kind result --body "boop done: <one line>"`
- Never commit `v6/boop/target/`. If reality deviates from the briefs, STOP
  and report via the same hail; do not improvise.
