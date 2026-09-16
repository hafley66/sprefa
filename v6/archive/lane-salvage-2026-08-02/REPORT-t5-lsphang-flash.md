# REPORT: v5 `dl --lsp` hangs on exit after answering shutdown

Worktree: `/Users/chrishafley/projects/sprefa-lanes/t5-lsphang/flash`
Branch: `lane/t5-lsphang-flash` (no commits made)

## Root cause

The LSP message loop in `src/lsp.rs` never handled the `exit` JSON-RPC
notification. In `run_lsp`, the `for msg in &connection.receiver` loop's
`Message::Notification` arm only matched `textDocument/didSave` and
`textDocument/didOpen`; the `exit` method fell through to `_ => None` and the
loop kept iterating.

Termination depended on two fragile lsp-server 0.7.9 side effects instead of
an explicit loop exit:

1. `shutdown` was the only explicit break, and only via
   `connection.handle_shutdown(&req)?` (src/lsp.rs:326-336). That convenience
   sends the response and then blocks in a 30s `recv_timeout` waiting for
   `exit`. If the `exit` delivery races the reader thread's zero-capacity
   channel rendezvous against this loop (or the client holds stdin open), the
   process does not terminate.
2. EOF termination came only from the stdio reader thread dropping its channel
   sender (`LspServerReader` in lsp-server's `stdio.rs`), which this loop never
   verified. That is a race, not a contract.

Consequence: a client that sends `shutdown -> exit` and closes stdin would, on
the racing interleaving, leave the process alive forever (no timeout on the
`exit` path). Disclosed at the LSP-diags arc (2026-07-29), not patched.

Pre-fix behavior (manual stdio, scripted session), reproduced standalone:

```
shutdown -> exit, stdin kept open : HANG (no exit in 10s)   [repro_drain: also HANG]
exit -> EOF (no shutdown)         : exit code 0             [spec wants 1]
EOF only                          : exit code 0             [spec wants non-zero]
```

## Fix (src/lsp.rs)

Minimal, no restructuring:

- Track `let mut shutdown_seen = false;` before the loop (src/lsp.rs:210).
- On `handle_shutdown` returning true, set `shutdown_seen = true; break;`
  (src/lsp.rs:326-336) — keeps the shutdown-first path.
- Handle the `exit` notification as an explicit loop terminator in the
  `Message::Notification` arm (src/lsp.rs:342-348): `if not.method == "exit" { break; }`.
  This removes reliance on the reader thread's channel-sender drop. EOF still
  terminates via the loop ending when the reader disconnects.
- After `drop(connection); io_threads.join()?;` return per the LSP 3.15 exit
  contract: `Ok(())` exit 0 if `shutdown` was seen first, else
  `Err(anyhow!(...))` exit 1 (src/lsp.rs:380-391).

The `--diag-db` read loop (`run_diag_db_mode`) had the same defect (notes only
handled `Request`, ignored `exit`) and got the equivalent fix (src/lsp.rs:499-523).

Resulting exit codes: shutdown-first -> 0; exit-without-shutdown -> 1; EOF
without shutdown -> 1.

## Regression test

`tests/it/lsp_exit.rs` (registered in `tests/it/main.rs` as `mod lsp_exit;`),
mirroring the existing `lsp_protocol.rs` stdio-framing pattern:

- `shutdown_then_exit_then_eof_terminates_with_code_0` — the named hang: sends
  shutdown, waits for the shutdown response (priming the message loop), sends
  exit, closes stdin, asserts the process exits with code 0 within a strict
  15s window (a hang fails it via timeout).
- `exit_without_shutdown_terminates_with_code_1` — exit-before-shutdown contract.

`EXIT_TIMEOUT = 15s`; `PRIME_TIMEOUT = 90s` (cold first-start on a fresh db can
be slow, kept separate from the hang window so startup slowness never trips the
hang assertion).

Pre-fix the exit-code test fails deterministically (`exit` without shutdown
returned 0, not 1); the hang is a load-dependent race and only manifests
intermittently, which is why the test carries the strict exit timeout
per the repo's macOS timeout convention.

## Validation

### 1. Regression test (post-fix)

```
$ nice -n 10 cargo test --test it lsp_exit 2>&1 | tail -8
     Running tests/it/main.rs (target/debug/deps/it-d1fa607b08030195)

running 2 tests
test lsp_exit::exit_without_shutdown_terminates_with_code_1 ... ok
test lsp_exit::shutdown_then_exit_then_eof_terminates_with_code_0 ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 1018 filtered out; finished in 2.07s
```

Pre-fix (src/lsp.rs reverted via stash, `lsp_exit` run against pre-fix build):

```
test lsp_exit::shutdown_then_exit_then_eof_terminates_with_code_0 ... ok
test lsp_exit::exit_without_shutdown_terminates_with_code_1 ... FAILED
  assertion `left == right` failed: exit without shutdown must be code 1
  left: 0   right: 1
test result: FAILED. 1 passed; 1 failed
```

### 2. Manual stdio transcripts

Pre-fix (release built from pre-fix source):

```
shutdown -> exit, stdin kept open : HANG (no exit in 10s)
shutdown -> exit -> EOF           : exit code 0   (escaped via EOF reader-drop race)
exit -> EOF (no shutdown)         : exit code 0   [spec: 1]
EOF only                          : exit code 0   [spec: non-zero]
```

Earlier clean reproduction (pre-fix binary, real stdio, `examples/lint-imports.dl`):

```
shutdown_exit_open     HANG (alive 12s, out_lines=2)  // shutdown response already sent
shutdown_exit_close    HANG (alive 12s, out_lines=2)
exit_only_close        exited code 0
eof_only               exited code 0
```

Post-fix (release built from fixed source):

```
shutdown -> exit -> EOF          : exit code 0 in 0.34s
exit -> EOF (no shutdown)        : exit code 1 in 0.04s
EOF only (no messages)           : exit code 1 in 0.03s
shutdown -> exit, stdin kept open: exit code 0   (no hang)
```

### 3. Clean final release build

```
$ nice -n 10 cargo build --release 2>&1 | tail -1
[=======================> ] 649/650: dl(bin)
$ ls -la target/release/dl
-rwxr-xr-x 1 chrishafley staff 67814192 Aug  2 00:42 target/release/dl
```

### 4. No regression in existing LSP integration tests

```
$ nice -n 10 cargo test --test it lsp_ 2>&1 | tail -3
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 988 filtered out; finished in 3.59s
```

## Files touched

- `src/lsp.rs` — the fix (main loop + `run_diag_db_mode` loop): exit-code flag,
  explicit `exit` notification break, post-loop exit-code decision, EOF
  termination.
- `tests/it/lsp_exit.rs` — new regression test.
- `tests/it/main.rs` — registered `mod lsp_exit;`.

> Note: commands >10s in this environment — `cargo build --release` and
> `cargo test` link steps took ~10-56s; the `lsp_exit` and `lsp_` test runs
> themselves finished in 2-4s. The LSP server's cold first-start on a fresh db
> also took multiple seconds (up to ~24s once); the test's priming timeout
> accounts for that.
