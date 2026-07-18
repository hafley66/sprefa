# Observability arc: invocation log, leveled logging, request-id access log

Status: LANDED 2026-07-18 (branch obs-logging).

## Incident

Six `dl` daemons were serially spawned and killed on one root in seven
minutes: an external agent's `dl --check` autostarts a daemon; every user
`kill` triggered a respawn and a fresh blank-slate cold rebuild. The killed
daemon read 499MB from disk on a 7.3MB corpus and beachballed the machine.
`dl daemon why` (`src/why.rs`) could state phase + cpu/io/rss of the DEAD
DAEMON from its on-disk trail; nothing recorded WHO spawned each process (no
ppid/ancestry anywhere), whether a one-shot/client invocation even ran,
leveled error/warn events, or per-request attribution. This arc closes that
gap in four layers.

## Design, four layers

### 1. Global invocation log (`src/invlog.rs`)

**Type signatures**

```rust
pub fn record_start(argv: &[String]) -> Option<i64>;     // row id, or None if disabled/failed
pub fn record_end(id: Option<i64>, exit_code: i32);       // finalize; no-op on None
pub fn recent(limit: usize) -> Vec<InvocationRow>;
pub fn lookup_by_pid(pid: u32) -> Option<InvocationRow>;
pub fn report_recent(limit: usize) -> String;              // `dl daemon invocations`
pub fn spawned_by_line(pid: u32) -> String;                 // the `why` join
```

**Pseudo-code body** (`record_start`): open-or-create
`<daemon_home>/invocations.db` (WAL, busy_timeout 5000ms) -> sweep rows older
than 14 days -> one `ps -eo pid=,ppid=,comm=` snapshot, walk `ppid` links up
to 5 hops for the ancestry string -> INSERT one row -> return `last_insert_rowid()`.
Every step best-effort: any `Option`/error collapses to `None`, never a panic,
never a slow path — a logging failure must not touch the CLI's own exit code
or latency.

**Storage layout**: one SQLite table `invocation` (schema in `src/invlog.rs`),
one process-wide db file. No index beyond `(pid)` and `(ts_start_ms)` — row
volume is one per `dl` invocation, bounded by the 14-day sweep.

**Instance lifetimes**: no long-lived handle — `open()` opens a fresh
`rusqlite::Connection` per call (`record_start`, `record_end`, every read).
This mirrors `jobq`'s `open_read_only` pattern rather than holding a
process-global connection: invlog calls are rare (once or twice per process
lifetime) so connection-open overhead is immaterial, and a fresh connection
per call sidesteps any cross-thread `Mutex<Connection>` plumbing for what is,
by construction, a write-once-read-rarely table.

**Sequence**: `cli::run()` calls `record_start(argv)` right after
`trace::init`, before any dispatch; `record_end` fires at both of `run`'s
exit paths (the `dispatch_subcommand` early exit before `std::process::exit`,
and after `dispatch_mode` returns). `std::process::exit` does not run `Drop`,
so both call sites are explicit, not an RAII guard — a SIGKILL between
`record_start` and either `record_end` leaves the row open (`ts_end_ms NULL`)
by construction; that open row IS the kill evidence, read back via
`dl daemon invocations` (flags it `KILLED` once `kill(pid, 0)` confirms the
pid is dead, `RUNNING` if still alive).

**Uniqueness**: `id INTEGER PRIMARY KEY` (SQLite rowid) is the only identity;
no dedup — every invocation gets its own row by construction (no coalescing,
unlike `jobq`'s per-key upsert).

### 2. Leveled logging (`src/trace.rs`, `daemon::init_daemon_tracing`)

**Type signatures**

```rust
pub fn init(is_daemon_foreground: bool);                    // CLI entry, every process
pub(crate) fn dl_log_layer<S>(home: &Path) -> impl Layer<S> + Send + Sync + 'static
    where S: Subscriber + for<'a> LookupSpan<'a>;
pub(crate) fn error_log_layer<S>(home: &Path) -> impl Layer<S> + Send + Sync + 'static
    where S: Subscriber + for<'a> LookupSpan<'a>;
```

**Pseudo-code body**: `init` builds a `tracing_subscriber::registry()` with
`dl_log_layer` + `error_log_layer` always attached; if `is_daemon_foreground`
it returns immediately WITHOUT calling `try_init` (see below); otherwise if
neither `RUST_LOG` nor `DL_TRACE` is set it `try_init`s the file-only
registry and returns; otherwise it also attaches a stderr `fmt::layer`
filtered by `DL_TRACE`/`RUST_LOG` and `try_init`s that. `daemon::run_daemon`
calls its OWN `init_daemon_tracing`, which composes the same
`dl_log_layer`/`error_log_layer` alongside an ALWAYS-ON stderr layer filtered
by `DL_LOG` (unchanged from before this arc).

**Why two separate generic functions, not one returning a tuple**: `S.with(a).with(b)`
requires `b: Layer<Layered<A, S>>`, not `Layer<S>` — a single generic call
site sharing one `S` across a tuple of two layers cannot satisfy that (hit
this as a real compile error while landing the arc). Two independent generic
functions, each its own monomorphization at its own `.with()` call site, do.

**Storage layout**: `<daemon_home>/log/dl.log` (level = `DL_LOG`, default
`info`) and `<daemon_home>/log/error.log` (fixed `warn`-and-up, independent
of `DL_LOG` — the "apache-style error log" always captures real problems
regardless of the operator's chosen verbosity). Each is a `RollingWriter`: one
`open(APPEND) -> write_all -> close` per COMPLETE formatted event (buffered
in a per-event `RollingGuard`, flushed once in `Drop` — see the doc comment
on `RollingGuard` for why: `tracing_subscriber`'s fmt layer can call
`Write::write` several times while formatting one event, and only a SINGLE
`write_all` per line is safe against two `dl` processes' writes interleaving
into corrupt output — same idiom `why.rs::append_line` already uses for
`why.jsonl`). Rotates to `<name>.1` (one generation) at 4MB, matching
`why.jsonl`'s own `ROTATE_BYTES`.

**Instance lifetimes**: no persistent writer handle across events —
`RollingWriter::make_writer()` is called once per tracing event by the fmt
layer and returns a fresh `RollingGuard` that lives exactly as long as that
one event's formatting, then drops (flushing). No background thread, no
buffered-writer-needs-a-flush-before-exit hazard (see build-vs-buy below).

**Sequence**: every `dl` process (one-shot CLI, daemon, `daemon start`
detach-and-return, `daemon why`, hooks) writes into the SAME two files under
`<daemon_home>/log/` — this is deliberate: the respawn-storm incident is
exactly the shape where many SEPARATE processes' history matters together.

**Uniqueness**: none needed — an append-only log has no identity constraint.

### 3. `why` enrichment (`src/why.rs`)

- `mark_boot` now also records `rayon::current_num_threads()` (sampled once
  at boot — the daemon's rayon pool size is fixed for the process lifetime by
  `apply_daemon_budget`, so a boot-time snapshot loses nothing a live sample
  would add, and a live per-sample thread count would need `ps -M -p <pid> |
  wc -l` on the 2s sampler cadence, which is not cheap enough to run that
  often).
- `report()` appends a "spawned by ..." line (ancestry + argv, from
  `invlog::spawned_by_line`) when the reported pid is confirmed dead, and a
  "recent invocations" section (`invlog::report_recent(5)`) UNCONDITIONALLY —
  even on the two early-return paths (no trail file, no boot marker) — via a
  `finish` closure wrapping every return. This matters because the
  respawn-storm shape is "many one-shot clients, no daemon ever settled,"
  which shows up ONLY in the invocation log, never in `why.jsonl` (which
  needs a daemon boot to exist at all).

### 4. Request-id access log (`src/reqid.rs`, `daemon::log_access`, `JobRow.req_id`)

**Type signatures**

```rust
pub fn next() -> String;                    // "<pid-hex>-<counter-hex>"
pub fn current() -> Option<String>;          // thread-local read
pub fn scope(id: &str) -> ScopeGuard;        // RAII thread-local set/restore

pub(crate) fn handle_request(d: &Arc<Daemon>, req: &Request, req_id: &str) -> Response;
pub(crate) fn log_access(surface: &str, req_id: &str, method: &str, root: Option<&str>,
                          ms: u64, ok: bool, bytes_in: usize, bytes_out: usize);
```

**Pseudo-code body**: both transports (`daemon_shell::uds::handle_connection`,
`daemon_shell::http::rpc`) mint a `req_id` via `reqid::next()` right after
parsing the request, time the `spawn_blocking(|| handle_request(...))` call,
and call `log_access` after it returns — one `[access]` line per request,
either surface. `handle_request` itself, at its very top, enters
`reqid::scope(req_id)`; the guard lives for the whole function body (RAII
across every early return) and is what lets ANYTHING synchronous on that same
thread for the rest of the call — an inline tick from `run_eval` (the
`eval`/`load mode=once` RPC path), a `JobRow` built inside dispatch — read the
id back via `reqid::current()`.

**Why a counter+pid, not a UUID**: "trace id" without an otel dependency, per
direction. `AtomicU64` counter + pid hex is unique enough within one
process's lifetime for grepping a shared log file; no new dependency.

**Storage layout**: `JobRow` gains `req_id: Option<String>`, persisted via an
idempotent `ALTER TABLE _job ADD COLUMN req_id TEXT` migration (guarded:
duplicate-column errors from a re-run are swallowed) rather than folded into
the base `SCHEMA`, so an existing `jobs.sqlite` from before this arc upgrades
in place. Every `JobRow` constructor (`tick`, `sink_drain`, `cold_extract`)
captures `reqid::current()` automatically — a call site never has to
remember to thread it through by hand.

**Instance lifetimes**: `reqid`'s id is a plain `String`, no lifetime beyond
normal ownership; the thread-local scope guard's lifetime is exactly one
`handle_request` call.

**Sequence + where the propagation reaches, precisely** (this is the "plumb
what is clean, document where it stops" bar from the task):

- REACHES: `crate::activity`'s `begin_tick`/`set`/`end_tick` tracing events
  (via a new `req_id` field on the `Activity` slot, set in `begin_tick` from
  `reqid::current()` at that moment) — so an inline tick triggered
  synchronously within a request's dispatch (today: `run_eval`, used by the
  `eval`/`load mode=once` RPC verbs) carries the request id into its
  `[tick] begin`/`[tick] phase done`/`[tick] end` lines, and into
  `why.jsonl`'s sample lines (which read the same activity snapshot). A
  `JobRow` constructed synchronously within a request's dispatch would
  likewise carry the id (no RPC verb does this today — see below).
- STOPS at a `tokio::spawn`/`spawn_blocking` boundary crossed by a DIFFERENT
  async task than the one holding the scope: thread-locals do not propagate
  across those boundaries automatically, and this crate's engine dispatch is
  already one synchronous call inside a single `spawn_blocking` closure — the
  scope guard is entered INSIDE that closure (in `handle_request`), so this
  is a non-issue for the paths that exist today, but a future RPC verb that
  spawns further async work off the dispatch thread would need to pass
  `req_id` explicitly, not rely on the thread-local.
- STOPS at the file watcher and poll timer: `JobRow::tick`/`sink_drain`
  constructed there capture `reqid::current()` on THOSE threads, which never
  entered a request scope — correctly `None`. Nothing was lost; that work
  truly is not caused by any client request.
- STOPS at a coalesced job rerun: a `dirty` reopen (jobq's `Requeue::Repending`)
  runs LATER, on a jobq worker thread, outside any live request's scope. The
  `req_id` stored on the row is a snapshot of whichever request MOST
  RECENTLY touched it (the coalescing UPSERT always takes the newest), not a
  durable causal link across reruns.
- No RPC verb in the current dispatch table enqueues a `jobq` job
  SYNCHRONOUSLY from within `handle_request` (ticks are triggered by the file
  watcher / poll timer, both outside any request), so today `JobRow.req_id`
  is `None` for every row in practice. The plumbing is there and correct for
  the day a verb does add a synchronous enqueue; documenting this rather than
  claiming false coverage.

## Build-vs-buy

### Rolling file writer (`src/trace.rs::RollingWriter`)

| candidate | fit | verdict |
|---|---|---|
| `tracing-appender` | Supports only TIME-based rotation (minutely/hourly/daily/never); its `Rotation::NEVER` gives a stable filename but no size bound at all, while any time-based rotation suffixes the filename with a date (`dl.log.2026-07-18-14`), which does not satisfy "dl.log" as a stable, greppable path. Its `non_blocking()` wrapper (the usual pairing) buffers writes on a background worker thread that must be given a chance to flush before the process exits — a real correctness risk for a MILLISECOND-lived one-shot `dl` CLI invocation, which could exit before the flush runs and silently lose its own log line. Using the blocking `RollingFileAppender` directly (skipping `non_blocking()`) avoids the flush race but still leaves the filename/rotation mismatch unresolved. | rejected: no size-based rotation, filename instability under time rotation |
| `flexi_logger` | Supports both time and size-based rotation with a stable base filename and numbered rotation, closer to the actual requirement. But it is designed around the `log` facade primarily and owns its own subscriber/writer setup; bridging it into `tracing-subscriber`'s `Layer`/`MakeWriter` model (which this crate's daemon subscriber composition already depends on — stderr layer + file layers on one `registry()`) adds real integration surface for a feature that is ~30 lines to hand-roll. | rejected: heavier integration than the problem needs |
| `log4rs` | Similar shape to flexi_logger — YAML/programmatic config-driven, built around the `log` facade, not `tracing-subscriber` native. Same integration-cost objection. | rejected: same gap as flexi_logger |
| hand-rolled, mirroring `why.rs`'s existing rotation | `why.rs::append_line` already solves an IDENTICAL problem (a multi-process-appended, size-rotated file, `why.jsonl`) in ~15 lines: check size, rename-to-`.1` if over cap, open-append-write-close. This arc's `RollingWriter`/`RollingGuard` is the same idiom, adapted to satisfy `tracing_subscriber::fmt::MakeWriter` (buffer per-event writes, flush once in `Drop`, so a multi-write-call event still produces one `write_all`). Zero new dependencies, stable filenames, size-based rotation, already-reviewed pattern in this codebase. | **picked** |

The tracing/tracing-subscriber pick itself is NOT re-litigated here — it was
already decided and recorded in `plans/2026-07-17-diag-stage-routing.md`
("Build-vs-buy: the logging/tracing crate"); this arc only adds file-writer
layers on top of that already-standing choice.

### Request id ("trace id without otel")

No library evaluated beyond confirming the explicit direction: "do NOT add
any otel dependency; just the id + consistent propagation." A counter+pid
string satisfies "short unique id... no uuid dep needed" from the same
direction. Nothing to build-vs-buy — the ask was explicitly for the smallest
possible primitive, not a tracing/correlation framework.

## What this arc did NOT do (explicitly out of scope)

- **eprintln migration**: the 223-site inventory from the R7 arc
  (`plans/2026-07-17-diag-stage-routing.md`) was NOT mass-converted. This arc
  converted only the seams it directly touched or that the spec named: the
  `ensure_singleton`/`spawn_detached` respawn-storm line, and left
  `src/watchdog.rs`'s wall-timeout `eprintln!`s and `src/lib.rs::run_watch`'s
  progress `eprintln!`s alone — both deliberately (documented in
  `docs/engine-loops.md`): the watchdog fires as the process is being killed
  by itself (a tracing event mid-rotate is a worse bet than a direct stderr
  write), and `--watch`'s lines are direct user-facing terminal output for a
  mode whose whole purpose is printing progress, not a log.
- **Log shipping / centralized aggregation**: `dl.log`/`error.log` are local
  files under `<daemon_home>/log/`, one pair per machine (shared across every
  `dl` process on that machine via `XDG_STATE_HOME`). No shipping to a
  central sink, no structured export beyond the rolling files themselves —
  out of scope; the incident this arc answers is single-machine forensics.
- **A live per-sample thread count** in `why.jsonl` — `mark_boot`'s
  `rayon::current_num_threads()` is a boot-time snapshot; see layer 3 above
  for why a live gauge was judged not cheap enough for the sampler's 2s
  cadence.
- **Restructuring the tick engine for request-id propagation** — explicitly
  told not to; the thread-local approach was chosen specifically because it
  requires zero changes to the tick engine's own call signatures, only to
  the `activity` slot and `JobRow`'s constructors.
- **Full plumbing of `req_id` across job reruns / cross-thread async
  boundaries** — see layer 4's "where it stops" list above; this is the
  `root attribution residual` item CLAUDE.md already tracks as accepted debt
  ("process-global approximation accepted, job-context plumbing is the true
  fix") — this arc IS that job-context plumbing, as far as it cleanly
  reaches without the restructuring above.

## Tests landed

- `src/invlog.rs` unit tests (round-trip, `DL_INVLOG=0`, dead-pid KILLED
  report) — serialized among themselves via a module-level `Mutex` (see the
  test module doc comment: `cargo test --lib` runs in parallel by default,
  and these are the only unit tests in the crate that mutate
  `XDG_STATE_HOME` in-process rather than sandboxing it on a spawned child).
- `src/trace.rs::rolling_writer_buffers_to_one_write_and_rotates` — the
  writer's own contract in isolation.
- `src/reqid.rs` unit tests — scope nesting/restore, id uniqueness.
- `tests/it/invlog.rs` — one-shot finished row, SIGKILL leaves an open row
  flagged `KILLED`, `dl daemon why`'s invocations section prints with no
  daemon ever run, `DL_INVLOG=0` end-to-end, `DL_LOG=info` produces `dl.log`
  with an INFO line and `error.log` with none.
- `tests/it/access_log.rs` — one HTTP `/rpc` request and one socket RPC each
  produce an `[access]` line in `dl.log` carrying a non-empty `req_id`.
- Updated `tests/it/setup_manifest.rs::uninstall_removes_journal_and_wiring_but_leaves_unowned_content`:
  the old assertion (`<state>/sprefa` fully gone after `uninstall`) is now
  false BY DESIGN — that directory is the invocation log + rolling logs' home,
  written by every invocation including `setup`/`uninstall` themselves, and
  deliberately NOT something `uninstall` should delete (that would destroy
  the very audit trail meant to help debug an uninstall gone wrong). Narrowed
  to assert the actual daemon-lifecycle artifacts (`daemon.pid`,
  `daemon.sock`, `roots.json`) are gone instead.

Full suite status at hand-off: `cargo test --lib` 555 passed / 0 failed / 1
ignored; `cargo test --test it` 881 passed / 0 failed / 15 ignored.
