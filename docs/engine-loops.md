# Engine loops inventory

Every long-running loop / background thread / async task loop in the `dl`
codebase, as of the observability arc (2026-07-18). Written as a companion to
that arc: for each loop, what wakes it, what it logs now, and how it exits —
the thing nobody could answer before `dl daemon why` + this arc's
leveled logging landed.

Runtime shape: the daemon's tokio shell (`src/daemon_shell/mod.rs`) is one
`new_multi_thread` runtime, `worker_threads(2)`, holding a single `ShellCtx {
rt, cancel: CancellationToken, job_notify: Arc<Notify>, broadcast_tx }`. Every
shell task selects on `ctx.cancel.cancelled()`; the engine itself is reached
only via `spawn_blocking` (sync tick engine, never async — the standing repo
law).

## Daemon shell loops

### 1. Per-root watcher — `watch_task`
`src/daemon_shell/watch.rs:24` (main loop `:83`, inner debounce loop `:121`)

- **Wakes on**: `tokio::select!` on `ctx.cancel.cancelled()` vs
  `tokio::time::timeout(1s, rx.recv())` on the notify-fed mpsc channel (OS fs
  event); a 120ms QUIET / 600ms MAX_WINDOW inner loop coalesces bursts.
- **Logs**: extensive pre-existing `tracing::info!/warn!/error!` coverage —
  watcher ready, watch errors forcing a full tick, program-edit reload,
  `.dl` discovery change, git ref advance, per-tick ok/error with path count.
  Plus, since this arc: every tick it drives emits `[tick] begin`/`[tick]
  end`/`[tick] phase done` from `crate::activity` (see below).
- **Exits**: returns on `ctx.cancel.cancelled()`, on `rx.recv()==None`
  (channel closed), or when the served root is dropped. Runs for the served
  root's lifetime; never a normal-completion exit.

### 2. Live job dispatcher — `worker_loop`
`src/daemon_shell/jobs.rs:39` (loop `:45`); spawned N-per-runtime by
`jobs::spawn` (`:23`)

- **Wakes on**: `tokio::sync::Notify` doorbell (`ctx.job_notify.notified()`,
  rung by `enqueue`) with a 500ms `tokio::time::sleep` safety backstop;
  claim/run/finish each hop is `spawn_blocking`.
- **Logs** (landed this arc): `[daemon] job claimed` (info, kind+key+root+
  req_id) on claim, `[daemon] job done -> ...` (info, kind+req_id+ms) or
  `[daemon] job failed: ...` (warn, kind+key+req_id+ms) on finish. This is the
  loop that also drains cold-start staging (`ColdExtract`) and `sink:{root}`
  drain jobs — there is no separate cold-drain loop; it rides this same
  dispatcher via job kind.
- **Exits**: `if ctx.cancel.is_cancelled() { return; }` checked at top of
  every iteration, or while parked on the `select!`.

### 3. Test-only OS-thread dispatcher — `worker_loop`
`src/jobq/dispatch.rs:76` (loop `:84`); spawned via `Dispatcher::spawn`
(`:41`, `std::thread::Builder::new().name("dl-job-{i}")`)

- **Wakes on**: `queue.claim()`; on `None`,
  `queue.wait_for_work(&mut last_gen, Duration::from_millis(500))` — a
  Condvar-style doorbell + 500ms timeout.
- **Not used by the live daemon** (explicit doc comment: "the live daemon no
  longer spawns this"), only `jobq`'s own unit tests exercise the
  claim/finish/coalesce/backoff SEMANTICS this shares with loop 2.
- **Logs**: `tracing::debug!`/`warn!` on job outcome/claim error (unchanged by
  this arc — it is not a live-process seam).
- **Exits**: `Arc<AtomicBool> shutdown` flag checked at top of loop; tests
  join explicitly via `Dispatcher::join`.

### 4. `why.rs` sampler — `start_sampler`
`src/why.rs:173` (loop `:176`), `std::thread::Builder::new().name("dl-why")`

- **Wakes on**: fixed sleep interval, `std::thread::sleep(Duration::from_secs(SAMPLE_SECS))`
  where `SAMPLE_SECS = 2`; every `IDLE_EVERY`th idle sample is downsampled.
- **Logs**: none via `tracing`/`eprintln` — it writes a JSON line to the
  on-disk `why.jsonl` trail via `append_line` directly (its own durable
  format, deliberately independent of the tracing subscriber so it survives a
  kill that raced a log flush). Since this arc: the boot line also carries
  `threads` (rayon pool size, sampled once) and `dl daemon why`'s reader joins
  a dead pid against `invocations.db` (`src/invlog.rs`) for the "spawned by"
  line.
- **Exits**: never — infinite `loop {}`, detached thread, ends only when the
  process exits.

### 5. Idle-timeout self-reap — `idle_task`
`src/daemon_shell/timers.rs:15` (loop `:17`)

- **Wakes on**: `tokio::time::interval(IDLE_TICK_SECS)` ticker, raced against
  `ctx.cancel.cancelled()`.
- **Logs**: `[daemon] job sweep: {e}` (warn), `[daemon] all roots idle {}min,
  exiting` (info).
- **Exits**: returns on cancel; on all-roots-idle it calls `shutdown_cleanup`
  then `std::process::exit(0)` — a hard process exit, not a loop return.

### 6. Poll / sink-drain timer — `poll_task`
`src/daemon_shell/timers.rs:47` (loop `:53`), scan body `poll_scan` (`:66`)

- **Wakes on**: `tokio::time::interval(secs)`, first tick consumed up front so
  the first scan waits a full interval; raced against cancellation.
  `poll_scan` runs on `spawn_blocking`.
- **Logs**: `[daemon] poll loop every {secs}s (@async drain via job queue)`
  (info, once pre-loop); inside `poll_scan`: root-gone/idle-probe/enqueue
  failures (warn).
- **Exits**: returns on `ctx.cancel.cancelled()`; never otherwise.

### 7. Subscriber-push pump — `spawn_subscriber_pump`
`src/daemon_shell/mod.rs:139` (loop `:143`)

- **Wakes on**: `tokio::select!` on `ctx.cancel.cancelled()` vs `rx.recv()` on
  the unbounded `BroadcastMsg` channel fed by tick methods (from
  `spawn_blocking` threads).
- **Logs**: none inside the loop (a dropped subscriber write is silently
  swallowed, best-effort push).
- **Exits**: returns on cancel, or when `rx.recv()==None` (all senders
  dropped).

### 8. UDS accept loop — `spawn_accept`
`src/daemon_shell/uds.rs:28` (loop `:38`)

- **Wakes on**: `tokio::select!` on cancel vs `listener.accept()` (OS socket
  accept event).
- **Logs**: `[daemon] adopt UDS listener into runtime: {e}` (error, setup),
  `[daemon] accept error: {e}` (warn).
- **Exits**: `break` on cancel; accept errors just log and continue.

### 8b. UDS per-connection loop — `handle_connection`
`src/daemon_shell/uds.rs:58` (loop `:61`), one task per accepted connection

- **Wakes on**: `read_frame(&mut reader).await` — one iteration per client
  request frame; NOT selected against `ctx.cancel` (no doorbell for graceful
  mid-request cancel).
- **Logs** (this arc): one `[access]` line per request via
  `daemon::log_access` (info) — request id, `surface=sock`, method, root, ms,
  ok, byte counts. Previously: only `[daemon] read error: {e}` (warn).
- **Exits**: returns on clean EOF (`Ok(None)`), read/write error, or after
  flushing a `shutdown` method's response (then calls `ctx.cancel.cancel()`
  and returns).

### 9. HTTP axum serve loop — `http::spawn`
`src/daemon_shell/http.rs:40`, `axum::serve(...)` at `:70`

- **Wakes on**: `axum::serve(listener, app).with_graceful_shutdown(cancel.cancelled())`
  — the accept loop lives inside the axum/hyper crate, not hand-written here.
- **Logs** (this arc): one `[access]` line per `/rpc` request via
  `daemon::log_access` (info) — request id, `surface=http`, method, root, ms,
  ok, byte counts. Unchanged: bind/listener-adoption failures (warn).
- **Exits**: graceful shutdown driven by `cancel.cancelled()`.

### 10. Signal handler — `spawn_signal`
`src/daemon_shell/mod.rs:169` (not a `loop{}`, a single `tokio::select!`)

- **Logs**: SIGINT/SIGTERM handler-unavailable (warn), SIGINT/SIGTERM —
  shutting down (info). Fires once, cancels the token, task ends (feeds loops
  1–9's shutdown).

### 11. Wall-clock watchdog — `arm_wall_watchdog`
`src/watchdog.rs:31` (one-shot thread, not a loop): sleeps `DL_MAX_WALL_SECS`
(default 300) once, then hard-exits.

- **Logs**: 4 `eprintln!` lines (`"[WALL TIMEOUT] ..."`, phase/detail/root,
  tick/elapsed, raise hint) — deliberately left as `eprintln!`, not migrated:
  this fires as the process is about to be killed by its own watchdog, and a
  tracing event routed through a file-writer that might itself be mid-rotate
  is a worse bet than a direct stderr write at the last possible moment.
- **Exits**: always `std::process::exit(124)`; armed only for one-shot CLI
  entries, never for `--watch`/`--lsp`/`--mcp`/`--hook`/daemon.

## Client-side / non-daemon loops

### 12. LSP daemon-push subscriber — `spawn_daemon_subscriber`
`src/lsp.rs:326` (loop `:339`), `std::thread::Builder::new().name("dl-lsp-subscriber")`

- **Wakes on**: blocking `rpc::read_frame(&mut s)` on the UDS socket
  subscribed to `diag_changed`.
- **Logs**: none in the loop.
- **Exits**: `return` on any non-`Ok(Some(_))` result (socket closed/error) —
  no cancellation token; thread dies with the connection.

### 13. LSP main pump — `run_lsp`
`src/lsp.rs:45` (loop `for msg in &connection.receiver` `:159`)

- **Wakes on**: `lsp_server` crossbeam-channel receiver (stdio JSON-RPC
  framing under the hood).
- **Logs**: `eprintln!("[lsp] daemon-push republish failed: {e}")` on
  republish failure — not migrated (editor-integration stderr surface, out of
  this arc's scope per CLAUDE.md's vscode Wave 4 item).
- **Exits**: channel closes when the LSP client disconnects stdio (editor
  shutdown) — no cancellation token, process-lifetime loop.

### 14. MCP stdio pump — `serve`
`src/mcp.rs:482` (loop `:487`, `while let Some(Frame::Rpc(msg)) = chan.recv()?`)

- **Wakes on**: `chan.recv()` — blocking read of one `Content-Length`-framed
  message off stdin.
- **Logs**: none in the loop.
- **Exits**: `while-let` ends when `recv()` returns `None` (stdin
  EOF/peer closed); IO errors propagate via `?` and abort the loop early.

### 15. CLI `--watch` loop — `run_watch`
`src/lib.rs:617` (loop `while let Ok(first) = rx.recv()` `:679`, inner
debounce `:688`)

- **Wakes on**: `std::sync::mpsc::Receiver::recv()` fed by a `notify` watcher
  callback; inner window `rx.recv_timeout(120ms)` capped at 600ms. The
  standalone foreground twin of loop 1 — same gate/debounce algorithm, but
  sync/std-thread instead of tokio, and outside daemon management.
- **Logs**: `eprintln!` lines (`"[watch] watching ..."`, config/refs watched,
  watch error forcing full re-tick, config reload) — not migrated to tracing
  (this is a one-shot CLI mode's own direct user-facing progress output, not
  a daemon internal; converting it would also silence it under the CLI's
  default no-subscriber-installed state, which is a UX regression for a mode
  whose entire purpose is to print progress to the terminal it's attached to).
- **Exits**: loop ends only if `rx.recv()` returns `Err`; otherwise runs
  until the process is killed (Ctrl-C — this arc's `invlog` row for the
  process stays open through that kill, same evidence pattern as any other
  killed invocation).

## What's now instrumented that wasn't

Every process (one-shot or daemon) now also carries:

- **Process start/end** (`src/cli/mod.rs::run`, info) — argv, pid, exit code.
  Fires once per process regardless of whether any loop above ever runs.
- **Invocation row** (`src/invlog.rs`) — a durable SQLite record per process,
  independent of `tracing` entirely; this is what survives a SIGKILL that
  raced a log flush (an open row IS the kill evidence).
- **Access log** (`daemon::log_access`, loops 8b/9, info) — one line per
  request, either transport, carrying a request id (`src/reqid.rs`) that
  propagates into `crate::activity`'s tick begin/end/phase events and into
  `JobRow.req_id` for jobs enqueued synchronously within a request's
  dispatch (see `plans/2026-07-18-observability.md` for exactly where that
  propagation stops).

## Not counted as separate loops

Spin/retry constructs, not background daemons: `src/rpc.rs` (sync frame
header parsing), `src/db.rs` (busy-handler retry, atomic CAS budget
reservation), parser/graph-algorithm inner loops, `await_quiescent`'s
client-blocking poll (bounded by request `timeout_ms`), `wait_ready`'s
CLI-side daemon-start poll (bounded by `CONNECT_TOTAL_TIMEOUT_SECS`), and any
bounded batch-apply cursor loop that terminates when rows are exhausted.
