# Boop Lane Create Await

## Context

Lane creation and completion waiting were separate coordinator commands. The
existing result-row reader already provides rc propagation, timeout exit 124,
and immediate reads of rows that have landed. Its entry point is
`run_lane_wait` in `v6/boop/src/main.rs:3668`.

The create path already owns lane identity, tmux identity, parent resolution,
spawn, route registration, and the completion hail at
`v6/boop/src/main.rs:1680`. Joining from that path removes the second command
while preserving the standalone `lane wait` command.

## Decisions

- `lane create` accepts `--await` and `--timeout <seconds>`. Zero seconds waits
  without a deadline, matching `lane wait`.
- The create branch calls `run_lane_wait` after dispatch. A second result-row
  reader was rejected because it would split rc, timeout, and row-selection
  behavior.
- `--timeout` requires `--await` on create. Silent unused timeout values were
  rejected.
- Dry-run returns after printing the spawn preview and await settings.
- The dispatch line contains the lane and tmux session and is flushed before
  the create process enters the waiter.
- Create has no detach-style CLI flag. No conflict declaration is needed for
  the current argument surface.

## Type Signatures

```rust
LaneCmd::Create {
    await_result: bool,
    timeout: u64,
    // existing create fields
}

struct LaneArgs {
    await_result: bool,
    timeout: u64,
    // existing create fields
}

fn run_lane_wait(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    timeout_secs: u64,
) -> Result<()>;
```

The create body resolves its parent and prepares the on-exit result hail. A
dry-run prints and returns. A live run dispatches the lane, flushes the route
line, and conditionally calls `run_lane_wait`.

## Instance Timeline

1. The coordinator process resolves the lane name, tmux session, parent, and
   result mailbox.
2. The detached tmux session starts and receives an on-exit hail command.
3. Create writes the route and dispatch message, then prints and flushes the
   lane and tmux route.
4. With `--await`, the coordinator process remains alive in `run_lane_wait`.
5. The lane exits, its shell writes one `kind=result` row containing `rc=N`,
   and the waiter exits with `N`.
6. If the requested deadline passes first, the waiter exits 124.

## Storage and Ordering

The route and result remain in the existing mail directory. Route
`registered_at` bounds the satisfying result rows to the current spawn. The
waiter reads the latest folded `kind=result` row whose sender or recipient is
the lane and parses its `rc=` token. Lane identity supplies the lookup key;
the route timestamp supplies current-spawn uniqueness.

Writes occur in this order: tmux spawn, route, dispatch row, visible route
line, result row. The await path starts after route and dispatch writes. Dry-run
performs none of those writes.

## Verification

- A real-binary integration test starts `lane create --await`, observes the
  flushed lane/tmux route, writes the current spawn's result row, and asserts
  that rc 17 reaches the caller.
- A real-binary integration test starts `lane create --await --timeout 1`
  without a result row and asserts exit 124.
- Existing standalone `lane wait` rc and timeout tests remain unchanged.
- Run `cargo test --no-fail-fast` from `v6/boop` three times. Record the two
  named pre-existing failures separately from any new failure.
- Inspect `boop beep lane create --help` and `boop beep lane --help`.
- Run a `create --dry-run --await --timeout 1` preview and verify immediate
  exit after the spawn line and route details.

## Staffing

One Codex lane implements in the dedicated
`.boop-worktrees/feature/boop-lane-create-await` worktree from base
`e70417d9`. The suite budget is three complete `cargo test --no-fail-fast`
runs in `v6/boop`.
