# stale-lanes-grade: three /tmp worktrees, one unmerged commit each

Repo /Users/chrishafley/projects/sprefa. origin/main tip after `git fetch origin` (10166672f or later).
Three worktrees, each `git log origin/main..HEAD` = 1 commit, no PR:
| card | worktree | branch |
|---|---|---|
| issues/extract-rev-pin-identity | /private/tmp/sprefa-extract-rev-pin-identity | fix/extract-rev-pin-identity |
| issues/restart-safe-retraction | /private/tmp/sprefa-restart-safe-retraction | fix/restart-safe-retraction |
| issues/watcher-auto-tick | /private/tmp/sprefa-watcher-auto-tick | feature/watcher-auto-tick |

For each, in this order:
1. Read the card whole (acceptance criteria, perf gates named on the card).
2. Read the commit diff. Decide: (a) complete vs the card, (b) partial, (c) superseded by something already on origin/main (grep origin/main for the same change; cite path:line).
3. If (a) or (b) worth finishing: create a fresh worktree off origin/main under /Users/chrishafley/projects/sprefa/.boop-worktrees/<branch>, cherry-pick the commit, resolve, finish the mechanical remainder if small (under ~150 lines), run the card's named gates plus the crate's own gate (`cargo test --features cli` in v6/sprefa-extract, `cargo test` in v6/sprefa-engine-rs, or the v6/justfile recipe the card names) twice, PR to hafley66/sprefa, merge if green (standing word), hail.
4. If (c) or not worth finishing: note the verdict with receipts on the card, close it or leave it open with the exact remainder listed, hail the verdict. Do not delete the /tmp worktrees; list them in the hail for Chris to prune.
Never edit the sprefa main tree. Provision each new worktree before its first commit: `cd v6/sprefa-extract && cargo build --release --features cli --bin extract`; `pnpm install --frozen-lockfile` in v6/tsv2 and v6/sprefa-store/js. No subagents. Follow /Users/chrishafley/projects/sprefa/CLAUDE.md style laws.

Reporting: `boop beep agent register stale-grader --parent sprefa-coordinator`; one hail per card verdict via `boop beep hail sprefa-coordinator --from stale-grader --body "..."`; `boop beep agent done stale-grader` at the end.
