# boop-lane-trail: a lane can die with no result row, no log, no trace

Card: /Users/chrishafley/projects/sprefa/issues/boop-lane-death-no-trail/item.md (read it whole first).
Repo: ~/projects/hafley-rs (crates/boop). Base: origin/main 8339d0e (run `git fetch origin` and use its tip).
Worktree: `git -C ~/projects/hafley-rs worktree add .boop-worktrees/fix/boop-lane-trail -b fix/boop-lane-trail origin/main`.
Never touch ~/projects/hafley-rs main tree files (another session has dirty files there). Never `cargo fmt` bare; `rustfmt --edition 2021 <file>` on files you edit only.

## Deliverables (the card's acceptance criteria)
1. Supervisor tees tracing to `~/.agent/lanes/<lane>/supervise.log` (append, line-flushed) in addition to the pane stderr. Tracing init lives at `crates/boop/src/main.rs:847-852` (`tracing_subscriber::fmt().with_writer(std::io::stderr)`). Use `tracing_appender` or a `Mutex<File>` writer via `fmt::Layer` composition; state candidate + choice in the PR body (build-vs-buy law).
2. The codex/opencode child's stderr goes to `~/.agent/lanes/<lane>/child.stderr` (find the spawn sites under `crates/boop/src/channel/`, `codex.rs:81` is the rpc write that broke).
3. Every supervisor exit path writes the result row (`crates/boop/src/supervise.rs:97,119,255-295`): add panic (`std::panic::catch_unwind` around supervise or a panic hook) and SIGHUP/SIGTERM (a `signal-hook` or `tokio::signal` handler, whichever the crate already depends on; check Cargo.lock before adding a crate) paths that write `rc=<code> (<reason>)`.
4. `boop beep lane list` shows a typed reason for a dead lane, never blank (dead-row rendering in `crates/boop/src/main.rs`, grep `dead`).
5. Attribution: `MAIN-TREE-COMMIT-SUSPECT` (`main.rs:4234,4537-4545`) blamed lane extract-module-plane-go for commits cd71912cd/36f56f008 made by lane dl6-bytes-target-lowering-2 in the shared sprefa main tree. Fix the attribution to use author time window AND branch reachability (`git branch --contains <sha>` matching the lane's branch), or say in the PR why that is not enough.
6. failure-modes entry in hafley-rs docs (find the existing ledger with `grep -rl 'failure' docs`), incident 2026-08-17, RCA from what the new trail can and cannot show retroactively (say plainly the two original deaths cannot be RCA'd from a trail that did not exist yet).

## Tests
Fail-pre-fix test per deliverable (sabotage receipt in the test header: neuter the fix, watch it fail, restore). Gate: `cargo test -p boop` three times in the worktree; report the numbers.

## Style laws
No em dashes. No eprintln in src (tracing only; `@eprintln-ok` waiver for CLI UX lines). Comment budget: constraints only. Banned words in prose and identifiers: provenance, substrate, load-bearing, regime, refusal.

## Reporting
Register: `boop beep agent register boop-trail-driver --parent sprefa-coordinator`. Post the PR to hafley66/hafley-rs with the gate numbers and sabotage receipts, then `boop beep hail sprefa-coordinator --from boop-trail-driver --body "PR #<n> posted: <one line>"`. Hail on a blocker too. Do not merge; do not install the binary into ~/.cargo/bin. On finish: `boop beep agent done boop-trail-driver`.
