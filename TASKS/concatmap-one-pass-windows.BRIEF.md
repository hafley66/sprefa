# concatmap rework: one call per window, concatScan kept, cursor replay

Repo: hafley-rs. Worktree base sha: ff2d67fd2cc9b78fba0bcc5ab2812e1f363a22b2 (origin/main).
FIRST ACTION: `git merge --ff-only ff2d67fd2cc9b78fba0bcc5ab2812e1f363a22b2`. If that fails, STOP AND REPORT.

Files you own: `crates/boop/src/concatmap.rs`, `crates/boop/src/query.rs`, boop CLI arg wiring for the concatmap subcommand, and boop tests for these. Touch NOTHING else. Forbidden: every other crate, `crates/boop/src/lane*`, store schema/DDL.

Full spec: sprefa issue `issues/concatmap-one-pass-windows/item.md` — read the whole Comments section, especially the SPEC CORRECTION note (2026-08-16). That note is the contract. Summary:

## The semantic

Two feeds, both kept, chosen by the rules file:

1. `feed:oneshot` = concatMap(window => ONE model call). Today `Rewriter::rewrite` routes OneShot into `passes_until_fixed` (concatmap.rs:193, 707-725), a fixed-point agreement loop. DELETE `passes_until_fixed` and the `--cap` flag (concatmap.rs:42, :144). Oneshot means exactly one call per window, no repeat, no convergence check.
2. `feed:chat` = concatScan. KEEP AS IS: resident conversation, `goal` seeds the accumulator, history carries state across windows (concatmap.rs:56-60). This arm was never the problem.

## The defects to fix

1. Cursor seeds at newest ts (`load_or_seed_cursor`, concatmap.rs:615), so mapping an existing conversation maps nothing. Add `--from-start` (cursor 0) and `--cursor <N>`. Default stays tail-only, but backfill must be a flag, never a hand-written state file.
2. In window mode `poll_once` advances the cursor from `turn_rows` timestamps, never from the window rows the caller's SQL returned. Advance from the returned window row ids/ts so replay runs from 0 until the view is exhausted, then keeps tailing. `Store::window_rows` (query.rs:46-79) is the contract: binds `:session`/`:session_id`/`:cursor`, returns `id`+`text`.
3. Default coalesce silently DROPS backlog (QUEUE_CAP, concatmap.rs:17). Flip the default to lossless (coalesce 0 / never drop); dropping is opt-in in the rules file.
4. The oneshot feed has no per-call timeout (the chat feed has CHAT_TURN_TIMEOUT=600s). Give the oneshot call the same bound; on timeout, kill the child, mark the bundle failed, continue the chain.
5. The retry ladder and the pre-built job queue never re-check `done/` markers mid-flight. Re-check the marker immediately before each attempt so planting a marker skips a poisoned bundle.

Keep the store SQLite-plain and the window SQL caller-owned. No new DSL, no schema change: a dl6 incremental view replaces this subscription later and must find nothing to unwind.

## Receipts (all three, in `crates/boop` tests or a scripted fixture run)

- (a) oneshot with `--from-start` over a pinned fixture conversation maps every window exactly once, exactly one model call each (count the calls with a fake harness adapter; the existing `FlakedChannel` test shows the pattern).
- (b) chat feed carries state across two windows: second output can reference the first (fake adapter recording the resident conversation is enough; do not call a real model).
- (c) a poisoned bundle test: a hanging fake call hits the timeout, is marked, and the next window still processes.

## Validation, run and paste output

```bash
cargo test -p boop
cargo build --release -p boop
```

Commit on your branch in the worktree. Never commit to main. Never push. Style: no `eprintln!` additions beyond the existing CLI-UX lines; comments state only constraints the code cannot show.
