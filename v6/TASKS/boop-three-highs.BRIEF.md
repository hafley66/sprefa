# boop-three-highs: respawn-loses-brief, install-clobber, mail-hook-inbox

Repo ~/projects/hafley-rs (crates/boop). Base: `git fetch origin`, origin/main tip (982472d or later, PR #12 lane-trail is in).
Worktree: `git -C ~/projects/hafley-rs worktree add .boop-worktrees/fix/boop-three-highs -b fix/boop-three-highs origin/main`.
Never touch the hafley-rs main tree. Never bare `cargo fmt`. No subagents. Verify every path:line below before relying on it (receipts rot); the code wins.
Cards (read whole): /Users/chrishafley/projects/sprefa/issues/{boop-tui-respawn-loses-brief,boop-install-clobber,boop-mail-hook-inbox}/item.md.
Interim shell hooks already live: /Users/chrishafley/projects/sprefa/.claude/hooks/boop-inbox.sh, boop-inbox-wait.sh, wired in .claude/settings.json (Stop + UserPromptSubmit). Read them; the native leg replaces them.

One PR per card, in this order, each on its own branch off origin/main (rebase the next onto the merged previous). Hail after each PR.

## PR 1: respawn re-feeds the brief (boop-tui-respawn-loses-brief)
Mechanism on the card: `crates/boop/src/channel/tui.rs` `type_and_submit_or_respawn` (~:134) retries only the current call's text after `reopen_window` (~:102); the brief is lost when a dead opencode window respawns without a captured conversation. Fix: the supervisor keeps the brief (LaneRun already loads it, `supervise.rs` "lane brief loaded"); on a respawn where no resume conversation was captured, re-feed brief + a one-line "resumed after window death, prior progress in the worktree" preface before the nudge. Fail-pre-fix test with sabotage receipt (there is an existing `start_turn_respawns_a_dead_agent_window` test in tui.rs to build beside). Failure-modes entry in hafley-rs docs/failure-modes.md.

## PR 2: install rail (boop-install-clobber)
`just install-boop` recipe (hafley-rs justfile; create if absent) that: refuses unless HEAD is an ancestor of origin/main (`git merge-base --is-ancestor HEAD origin/main`) and the tree is clean; builds `cargo build --release -p boop`; installs by `rm` then `cp` then `codesign --force --sign - ~/.cargo/bin/boop` (plain cp gives Killed: 9 on macOS, receipt on the card); stamps the sha into `boop --version` (build.rs or env at compile: `git rev-parse --short HEAD`, dirty flag). `boop beep lane create` prints the installed binary's sha in its first log line so a lane death can be tied to a binary. Test: version string carries the sha; recipe refuses on a dirty tree (shell test or a rust test invoking the recipe's check script). Do NOT run the install yourself.

## PR 3: native mail push for claude coordinators (boop-mail-hook-inbox)
`boop adopt` (find the subcommand in main.rs) for a claude-kind coordinator installs two hooks into the project `.claude/settings.json` (idempotent, `--uninstall` path): Stop hook = `boop inbox drain --as <me> --hook stop` returning `{"decision":"block","reason":"<mail>"}` only when unread mail exists (else exit 0 silent); UserPromptSubmit hook = `boop inbox drain --as <me> --hook prompt` printing the mail as context. `boop inbox drain` is a new subcommand: reads unread rows addressed to `--as` from ~/.agent/mail/bus.ndjson, marks them acked (same ack path `boop beep message ack` uses), prints. Port the semantics of the interim shell hooks exactly, including the drained-id ledger; then delete nothing in sprefa (I swap the settings after merge). e2e: hail during a simulated long turn, assert it arrives once and whole at Stop, and `tmux capture-pane` on the coordinator pane shows no keystrokes. failure-modes entry: keystroke-injection era closed.

## Gate
`cargo test -p boop` three times per PR, clippy clean, no eprintln in src, no em dashes, banned words (provenance, substrate, load-bearing, regime, refusal). Build-vs-buy note in each PR body where a library could apply (json parsing for hooks: serde_json already present).

## Reporting
`boop beep agent register boop-highs-driver --parent sprefa-coordinator`. Post PRs to hafley66/hafley-rs; merge if green (standing word); hail `boop beep hail sprefa-coordinator --from boop-highs-driver --body "MERGED PR #n: ..."` per PR, or blocker. `boop beep agent done boop-highs-driver` at the end.
