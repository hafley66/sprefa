# Infra library adoption: queue/scheduling, HTTP/RPC, daemon supervision

Status: research complete (three parallel web-research agents, 2026-07-18),
verdicts written, awaiting user decision before any adoption commit.

Standing law (a1f049ff): scheduling, job queue, HTTP serving, daemon
lifecycle/supervision, and logging/telemetry run on established Rust libraries
or the OS service manager. This plan is the candidate-by-candidate written
analysis that law requires before any adoption or migration commit.

Prior art already in-repo: plans/2026-07-18-resource-aware-scheduler.md
section 2 surveyed apalis (2h), effectum (2i), underway/aide-de-camp/sqlxmq/
fang/hatchet (2j), DICE/Bazel (2k) and verdicted build-on-jobq. That verdict
is superseded by the law; the survey's factual findings (effectum weight =
scalar admission, apalis reenqueue_orphaned already borrowed at jobq
mod.rs:49, no crate ships read/write-set conflict scheduling) remain valid
inputs and are re-litigated below under the new posture: the question is no
longer "does a crate ship R2/R3" but "which crate carries the durable-queue
layer, with R2/R3 as a thin layer on top of it".

## 1. Candidate analysis — job queue / scheduling

Hard constraints: embeddable in-process, SQLite (or own local file)
persistence surviving SIGKILL, SQL-introspectable for `dl daemon why`,
priorities, dedup/coalesce, cancellation, concurrency caps, tracing.

### 1.1 Tier 1 — embeddable + SQLite-persistent

**effectum** — 0.7.0 (2024-07-23), MIT/Apache-2.0, solo maintainer
(dimfeld), 48 stars, last commit 2024-07-23 (2 years stale), 95 commits.
tokio + rusqlite ^0.31 + deadpool-sqlite, depends on `tracing`.
Fit: `priority()`, `weight()` (scalar admission against worker
max_concurrency), `run_at()`, retries/backoff, `timeout()`, heartbeat,
recurring-job CRUD with cron, `JobRecoveryBehavior` for jobs found running
at startup (the SIGKILL story), checkpoint/resume.
Misfits: no dedup/unique for regular jobs (coalescing = app code via
`get_jobs_by_name` + `update_job`); running jobs cannot be cancelled
(pending-only cancel per Queue docs); rusqlite ^0.31 pins libsqlite3-sys
0.28, link-incompatible (`links = "sqlite3"`) with the repo's rusqlite
0.40.1 — unforked adoption forces the whole repo down to rusqlite 0.31.
Integration: same rusqlite stack as dl, own sqlite file beside the repo
db, workers as tokio tasks on a QoS-demoted runtime, why reads its jobs
table with plain SQL. Realistically requires owning a maintained fork.

**apalis 1.0-rc.9 + apalis-sqlite 1.0.0-rc.8** — rc.9 2026-05-06 (0.7.4
stable 2025-11-18), MIT / MIT+Apache-2.0, 928k downloads, 1.3k stars,
2 open issues, geofmureithi dominant (bus factor 1). Very active:
rc.1 2025-12-23 → rc.9 2026-05-06; `idempotency_key` migration 2026-05-06.
sqlx ^0.8.6 (SqlitePool), own migrations (jobs+workers tables, priority
column, stats indexes, worker_started_at, metadata).
Fit: integer priorities, `run_after`, retries+backoff, idempotency keys
(dedup/coalesce primitive), heartbeat + orphan recovery (dead-worker
detection = SIGKILL survival), `SqliteStorageWithHook` (sqlite update
hooks, event-driven dispatch instead of polling), `SharedSqliteStorage`,
per-worker concurrency, tower middleware (retry/timeout/rate-limit/
catch-panic), tracing on by default, OTel layers in rc.8/9,
`apalis-workflow` DAG/sequential workflows (maps to future
readiness/dependency rows), apalis-board UI.
Misfits: cancellation of a RUNNING job not first-class (worker-level
`WorkerContext::stop()` + handler-side `AbortError`; issue #139 is still
the job-mutation reference); rc API churn was real rc.1→rc.9; sqlx-sqlite
0.8.6 pins libsqlite3-sys ^0.30.1, incompatible with rusqlite 0.40 in one
binary — coexistence means pinning dl to rusqlite 0.32.x or waiting on
sqlx.
Integration: separate queue.db; daemon spawns an apalis `Monitor` on a
capped runtime; tick loop pushes with idempotency keys; why queries
apalis-sqlite tables directly.

**fang** — 0.11.0 (2026-07-02; prior stable 0.10.4 in 2023, bursty
cadence), MIT, 716 stars. sqlx ^0.8, `fang_tasks` table, `uniq()` dedup,
cron + one-shot, retries/backoff, per-type workers, panic recovery.
Misfits: no priority support anywhere in the documented API; no per-task
cancel/remove documented; same libsqlite3-sys pin issue; 3-year release
gap is a weak maintenance signal despite the fresh release.

**aide-de-camp(-sqlite)** — 0.2.0 (2022-12-18), sqlx ^0.6 (two majors
behind), no core release since 2022, own docs admit "possible that a
single job gets sent to two runners" on SQLite, no cancel API. Dead
project with a correctness caveat in its own README.

### 1.2 Tier 2 — wrong storage/architecture

| Crate | Version / date | Disqualifier |
|---|---|---|
| underway | 0.2.0, 2025-07-16 | Postgres locking + LISTEN/NOTIFY by design; no SQLite backend exists or is planned. |
| sqlxmq | 0.6.0, 2025-05-25 | "PostgreSQL as a backing store" by design. |
| backie | 0.9.0, 2023-07-12 | Diesel+Postgres; dormant 3 years. |
| ocypod | 0.8.0, 2022-08-05 | Redis-backed standalone HTTP server (two external processes); dormant 4 years. |
| faktory-rs | 0.13.1, 2025-07-06 | Well maintained (jonhoo) but client bindings to a separate Go work server. |
| rust-task-queue | 0.1.5, 2025-06-19 | Redis fundamental to core design. |

**gaffer** (0.2.0, 2021-10-26, 46 stars, no release ~5 years) is the
closest feature mirror of bespoke jobq: priority ordering, job merging
(exactly the coalesce need), key-based concurrent exclusion, priority
throttling (idle threads reserved for high priority). Rejection: purely
in-memory — SIGKILL survival and post-mortem why are impossible — and
abandoned since 2021, bus factor 1. Design worth stealing; crate not
adoptable.

### 1.3 Tier 3 — cron-slice-only

tokio-cron-scheduler 0.15.1 (active, persistence = Postgres/NATS only),
clokwerk 0.4.0 (dormant 3.5 yrs, in-memory), delay_timer 0.11.6
(in-memory). All leave the entire queue bespoke; dl's tick loop already
owns the recurring-trigger slice.

### 1.4 Composition baseline

tokio + CancellationToken + Semaphore + own SQLite table = the incumbent
bespoke jobq. Maximal fit, but it is precisely what the law prohibits
extending. Listed as baseline only.

### 1.5 Verdict

1. **apalis 1.0-rc + apalis-sqlite** — only candidate simultaneously
   active, SQLite-persistent in-process, priority-aware, dedup-capable,
   orphan-recovering, tracing-native, SQL-introspectable; apalis-workflow
   lines up with the readiness/dependency-rows need. Accepted risks: rc
   churn, sqlx/rusqlite libsqlite3-sys pin, weak running-task
   cancellation, bus factor 1.
2. **effectum 0.7.0 (forked)** — architecturally the best rusqlite-daemon
   match, but adoption means owning a fork (2 years dormant, rusqlite 0.31
   pin, no dedup). Fork cost bounded (95 commits) but converts buy into
   buy-then-adopt-maintenance.
3. fang as fallback if rc churn intolerable and a fork unwanted; losing
   priorities is a direct regression vs bespoke jobq.

### 1.6 Prototype open questions

1. libsqlite3-sys unification: can dl pin rusqlite to the 0.30.x line
   (rusqlite 0.32) without losing needed features, or is sqlx 0.9/fork
   needed? Gates apalis and fang equally.
2. req_id cancellation as apalis tower middleware (cancel-flag SQL row
   checked before+during execution) — acceptable latency?
3. Does apalis-sqlite retain completed/failed rows with timings long
   enough for `dl daemon why`; is retention configurable?
4. WAL contention between queue.db update-hook storage and the engine's
   rusqlite writes; measure fsync/lock interference.
5. Confirm handlers run on threads the daemon controls (dedicated runtime,
   `on_thread_start` QoS demotion).
6. rc-to-1.0 API distance; mitigation = pin rc.9 + vendored lockfile.
7. Resource-aware scheduling (per-shard cost, write-volume budget) exists
   in no candidate; verify it lives as an admission layer above the queue
   (deciding what to push, not how workers pull) in both shapes.

## 2. Candidate analysis — HTTP + UDS RPC consolidation

Requirement set: one server code path (erases the public/no-daemon split),
localhost TCP + UDS off the same handler layer, streaming/long-poll for watch
subscriptions, graceful shutdown, request-scoped tracing spans, small
footprint, sync core untouched (async stays in the shell).

### 2.1 Candidate matrix

| Candidate | Latest ver (date) | License | Dep weight | UDS story | Streaming | Graceful shutdown | tracing/tower | Maintenance |
|---|---|---|---|---|---|---|---|---|
| axum | 0.8.9 (2026-04-14) | MIT | ~18 direct, ~274K SLoC tree, MSRV 1.80 | First-class: `tokio::net::UnixListener` implements `axum::serve::Listener`; official unix-domain-socket example | SSE built in, `Body::from_stream` | `serve(...).with_graceful_shutdown(fut)`, any Listener | Native tower; `tower_http::trace::TraceLayer` | 33M dl/mo, #1 HTTP server on lib.rs, tokio-rs org |
| actix-web | 4.14.0 (2026-06-21) | MIT/Apache-2.0 | ~568K SLoC tree, MSRV 1.88 | First-class `bind_uds`/`listen_uds` | SSE via body streams | `shutdown_timeout`, `shutdown_signal` | Own middleware; tracing via third-party `tracing-actix-web` | Active, 153 releases |
| poem | 3.1.12 (2025-07-28) | MIT/Apache-2.0 | ~46 direct, ~716K SLoC tree | First-class `UnixListener` + `Listener::combine` (one Server over TCP+UDS) | SSE built in | `run_with_graceful_shutdown` | Built-in Tracing middleware; tower-compat feature | ~12 months since release; repo alive, 4.4k stars |
| warp | 0.4.3 (2026-05-04) | MIT | hyper-v1, moderate | Adapter only via `Server::incoming()` custom acceptor | SSE, streams | `.graceful(fut)` | `warp::trace`, `warp::service` for tower | Single maintainer (seanmonstar) |
| rouille (sync) | 3.6.2 (2023-04-24) | MIT/Apache-2.0 | 14 direct, 5.5K SLoC | Absent: `ToSocketAddrs` only | Long-poll pins a pool thread | `stoppable`/`join` (up to 1s stop latency) | Nothing; hand-roll | Last release 3+ yrs, Rust 2015 edition |
| tiny_http (sync) | 0.12.0 (2022-10-06) | MIT/Apache-2.0 | 5 deps | Yes (`ListenAddr` unix, 0.12) | Chunked; thread-per-blocked-request | `unblock`/drop, manual drain | Nothing above socket level | No release since 2022; 18 unmerged PRs |
| hyper direct | 1.10.1 (2026-05-29) | MIT | small | Trivial (`serve_connection` on any IO; hyperlocal 0.9.1) | Manual | Manual (hyper-util graceful) | Manual wiring | Very active |
| salvo | 0.95.0 (2026-07-15) | Apache-2.0 | ~1M SLoC tree, MSRV 1.94 | First-class `UnixListener`, joinable listeners | SSE | `serve_with_graceful_shutdown` | Own middleware + tower compat | Weekly releases; 83 breaking of 195 |
| tarpc (RPC) | 0.37.0 (2025-08-10) | MIT | tokio + tokio-serde | First-class `serde_transport::unix` | Via transport | Manual | tracing internal | ~2 releases/yr |
| jsonrpsee (RPC) | 0.26.0 (2025-08-11) | MIT | Parity stack (hyper, soketto) | Absent: HTTP+WS only; UDS issue #5 never landed | WS subscriptions | Stop handle | tower-integrated | Active (Parity) |

### 2.2 Rejections (with evidence)

- **rouille**: no UDS constructor at all (fails the two-transport requirement
  without patching the crate); dormant since 2023-04 on Rust 2015 edition.
- **tiny_http**: UDS works but nothing exists above the socket — routing,
  middleware, spans, SSE stay bespoke, which is exactly the code the law
  targets. Dormant since 2022-10, 18 unmerged PRs.
- **actix-web**: full coverage incl. `bind_uds`, but drags actix-rt/System,
  a multi-worker model sized for high throughput, ~2x axum's dep tree, and
  tracing only via a third-party crate. Adds nothing over axum needed here.
- **warp**: healthy post-0.4, but UDS is adapter-grade and it is one
  maintainer's side project on the same hyper axum sits on.
- **salvo**: covers everything but heaviest tree of the field (~1M SLoC),
  MSRV 1.94, 83 breaking releases of 195.
- **hyper direct**: UDS trivial but routing/middleware/response plumbing
  would be rebuilt by hand; axum IS that layer, bought.
- **tarpc**: good typed UDS RPC, zero HTTP — forces a second server stack
  for status/panel, contradicting consolidation by construction.
- **jsonrpsee**: no UDS transport (issue #5 open since inception); JSON-RPC
  envelope + soketto buy nothing over plain JSON routes for one local client.

### 2.3 Sync vs tokio given the sync core

Sync candidates lose on the matrix, and tokio does not contaminate the core:
the shell owns a current-thread tokio runtime (fits the thread-budget law,
sufficient for a handful of local clients) and crosses into the sync tick
engine via channel or `spawn_blocking`. The core stays sync either way.

### 2.4 Verdict

1. **axum 0.8** — one `Router` = the single handler layer; two thin
   listeners (`axum::serve(tcp, app.clone())` + `axum::serve(uds, app)`)
   sharing one shutdown token. `TraceLayer` for request spans, SSE for watch
   subscriptions, RPC = plain JSON routes reachable over either transport.
   Optional `tokio-listener` 0.5.2 (`axum08` feature) for a single
   runtime-selectable listener.
2. **poem** — runner-up; `Listener::combine` is the only literal
   one-server-two-transports API and tracing middleware is built in, but
   release cadence (12 months) and ecosystem size rank below axum.

Shape: one framework, one handler layer, two thin listeners. UDS carries
HTTP; no separate RPC framework.

### 2.5 Prototype open questions

- ConnectInfo divergence (TCP `SocketAddr` vs `UdsConnectInfo`) in shared
  handlers.
- Drain semantics for long-held SSE/long-poll under
  `with_graceful_shutdown` (need own cancellation to avoid blocking drain?).
- Socket file lifecycle: stale-socket unlink at boot, mode 0600, parent dir.
- Runtime flavor (current_thread vs multi) vs `apply_daemon_budget` caps.
- Core bridge: mpsc into tick loop vs `spawn_blocking` + engine mutex;
  backpressure for watch subscriptions.
- CLI client over UDS: hyperlocal 0.9.1 (slow-moving) vs raw hyper on
  `UnixStream`.
- Measured binary/compile delta vs bespoke path (`cargo bloat` before/after).

## 3. Candidate analysis — daemon lifecycle / supervision

Requirement set: user-level (no root), macOS primary / Linux secondary,
survive-and-explain after SIGKILL, nothing-seizes-the-machine budgets,
stale-binary detection, single-instance enforcement, CLI start/stop/status.

### 3.1 macOS launchd user LaunchAgent (primary path)

Plist at `~/Library/LaunchAgents/com.sprefa.dl.plist`, `launchctl` in the
`gui/$UID` domain. Verified semantics (launchd.plist(5)):

| Key | Behavior |
|---|---|
| `KeepAlive` dict | `SuccessfulExit: false` = restart only on non-zero exit; `Crashed: true` = restart on crash signals. Conditions OR. Clean `dl daemon stop` stays stopped, crashes respawn. |
| `ThrottleInterval` | Default 10s; minimum-runtime semantics, NOT backoff. Job that ran 3s gets respawn pushed 7s; job that ran >10s respawns immediately regardless of exit code. No exponential backoff exists. |
| `ProcessType: Background` | Darwin background classification (same clamp as `taskpolicy -b` / `PRIO_DARWIN_BG`): lowest scheduling tier, efficiency-cores-only on Apple silicon, throttled I/O. Replaces the in-process QoS + nice + IOPOL_THROTTLE trio in one key. |
| `Nice`, `LowPriorityIO` | Direct plist replacements for the in-process `nice()`/`setiopolicy_np()` calls. |
| `HardResourceLimits.CPU` | setrlimit CPU seconds (SIGXCPU), not percent. Confirmed: launchd has NO CPU-percent cap; macOS has no cgroup equivalent. Percent-shaped budgeting cannot move to launchd. |
| `ExitTimeOut` | SIGTERM-to-SIGKILL window on stop. |
| `StandardOutPath`/`StandardErrorPath` | Log files, unrotated (newsyslog or `tracing-appender`). |
| `AbandonProcessGroup` | Default kills whole process group on job death. |

CLI mapping: start = write plist + `launchctl bootstrap gui/$(id -u) ...`
(+`kickstart`); stop = `bootout`; restart = `kickstart -k` (also the
stale-binary remedy, `-p` prints new pid); status = `launchctl print`
(shows last exit status; format explicitly not API-stable, keep liveness on
the daemon's own heartbeat). `load`/`unload` are deprecated syntax.

Crash observability: `launchctl print` last-exit-status, `log show`
launchd predicates for spawn/throttle/exit lines, ReportCrash `.ips` in
`~/Library/Logs/DiagnosticReports`. Second witness beside `dl daemon why`;
requirement (a) stays in-process.

Cannot do: CPU-percent caps, thread caps, memory caps, exponential
backoff, stale-binary detection, readiness protocol. UX caveat: macOS 13+
surfaces the agent in System Settings > Login Items with a one-time
notification.

Critical integration rule: launchd wants a non-forking child. The
self-forking daemonization code becomes a bug under launchd (it would track
the dead parent). Daemon mode becomes run-in-foreground, log to
stderr/tracing.

### 3.2 Linux systemd user unit (parallel path)

`~/.config/systemd/user/dl.service`, `systemctl --user`.

- `Restart=on-failure` + `RestartSec` + `StartLimitIntervalSec`/`Burst`:
  strictly better respawn semantics than launchd (real backoff + burst cap).
- `Type=notify` + sd_notify readiness; crate `sd-notify` 0.5.0 (2026-03,
  12.4M downloads), no-op shim on macOS.
- stdout/stderr to journald; `tracing-journald` for structured fields.
- Resource control catch: `CPUQuota` (hard percent) and `CPUWeight`/
  `IOWeight` need cpu/io cgroup controllers delegated to `user@.service`;
  most distros delegate only memory+pids by default. Enabling needs a
  root-installed `Delegate=` drop-in. `TasksMax`/`MemoryMax` work out of the
  box; treat `CPUQuota` as best-effort, keep in-process budget as fallback.
- `ExecMainStatus`/`Result` properties beat launchctl print for exit reasons.

### 3.3 Crates

| Crate | Ver / date | License | Downloads | Verdict |
|---|---|---|---|---|
| service-manager | 0.11.0, 2026-02, active CI | MIT/Apache-2.0 | 460K | ADOPT: install/uninstall/start/stop across launchd+systemd, `ServiceLevel::User`, and `ServiceInstallCtx.contents` accepts full hand-authored plist/unit text so KeepAlive dict/ThrottleInterval/ProcessType/Nice/CPUWeight/Restart= are all reachable. Write the KeepAlive dict via contents (its RestartPolicy enum degrades launchd to boolean KeepAlive). |
| daemonize | 0.5.0, 2023-02, unmaintained (zellij migrated off; `daemonix` fork fixes a `privileged_action` unsoundness) | MIT/Apache-2.0 | 15.7M | Obsolete under service-manager ownership; keep out. |
| daemonize-me / fork | 2.0.4 / 0.10.0 | BSD-3 / - | 327K / 5.5M | Same category; only relevant to a no-service-manager fallback. |
| auto-launch | 0.6.0, 2026-01 | - | 4.4M | Login registration only, no start/stop/status; subset of service-manager. Skip. |
| single-instance | 0.3.3, 2021-12 | MIT | 820K | Stale since 2021. Skip. |
| fd-lock | 4.0.4, 2025-03 | - | 53.7M | ADOPT for single-instance: advisory fd lock, kernel-released on SIGKILL (no stale pidfile). Lock a file beside the SQLite db; doubles as the who-holds-the-db witness for `dl daemon why`. |
| named-lock / pidlock | 0.4.1 / 0.2.2 | MIT | 4.2M / 60K | Work, but fd-lock is simpler and far more used. |
| duct | 1.1.1, 2025-11 | MIT | 33.2M | Child-tree kill for CLI side/tests; not for the daemon once launchd owns it. |
| command-group | 5.0.1, 2023-11 | Apache-2.0/MIT | 6.6M | Superseded by same author's `process-wrap` 9.1.0 (2026-03, 9.1M dl, active) if dl spawns worker subprocesses. |
| sd-notify | 0.5.0, 2026-03 | - | 12.4M | ADOPT (Linux readiness/watchdog; shim on macOS). |

launchd/systemd already enforce one instance per label/unit per user; the
fd-lock is belt-and-suspenders against `dl serve --foreground` beside the
agent.

### 3.4 Keep-it-in-process supervisor loop

Crate landscape is thin: `rust_supervisor`, `task-supervisor` (tokio tasks,
not OS processes), `supertrees` (self-declared experimental),
`ractor-supervisor` (drags an actor framework). A parent-supervisor design
re-creates the bespoke respawn/pidfile/budget code in two processes. Only
sensible as degraded fallback where no service manager exists (CI
containers), and there run-foreground-under-the-caller is simpler.

### 3.5 Verdict

1. launchd/systemd own: daemonization, respawn, start/stop/restart,
   single-label enforcement, log routing, nice/IO priority (macOS),
   CPU/IO/tasks weighting (Linux). Deleted bespoke code: self-fork, respawn
   loop, most pidfile handling, the macOS nice/IOPOL calls.
2. `service-manager` 0.11.0 is the one crate to adopt, passing hand-authored
   plist/unit text through `contents`; `dl daemon start/stop/restart/status`
   become thin wrappers with raw `launchctl`/`systemctl` debug fallbacks.
3. Stays in-process permanently: thread caps (rayon/tokio pool sizing),
   CPU-percent budgeting on macOS (no OS mechanism; darwinbg is a class,
   not a cap), stale-binary detection (exe mtime vs repo build, remedy
   `kickstart -k`), the `dl daemon why` trail, the fd-lock witness.
4. Drop entirely: daemonize-family crates, bespoke respawn logic,
   auto-launch, single-instance.

### 3.6 Prototype open questions

- Does `ProcessType=Background`'s efficiency-core-only clamp make first-run
  rebuild unacceptably slow? Alternatives: `Nice`+`LowPriorityIO` without
  ProcessType, or per-phase self-toggle of `PRIO_DARWIN_BG`.
- Is `launchctl print` parsing stable across macOS 14-26, or does status
  stay on the daemon heartbeat (recommended; `gui/` bootstrap quirks
  reported on macOS 26)?
- ThrottleInterval minimum-runtime semantics vs crash storms: a daemon
  crashing 11s after start respawns instantly forever; does `dl daemon why`
  need a respawn-count alarm?
- BTM/Login Items notification wording acceptable UX?
- Linux: ship with or without the root `Delegate=` drop-in (`CPUQuota`
  silently degrades without it)?

## 4. Moot-list: in-flight and queued work each adoption obsoletes

### 4a. Queue adoption moots / reshapes

- **db-seam batch A4 (jobq/daemon slice)** — plans/2026-07-18-db-seam-migration.md
  section "File partition", row A4: `effect.rs 16/18/0 · jobq/mod.rs 2/8/2 ·
  daemon.rs 0/0/2`, 48 sites. If jobq's SQL moves into an adopted queue crate's
  own storage, migrating jobq/mod.rs's 10 rusqlite/conn sites (plus
  jobq/tests.rs 0/5/1 inside B2) onto Db is wasted motion. Effect.rs's 34
  sites stay regardless (engine effects, not queue plumbing). Action: A4
  splits — effect.rs proceeds, jobq/mod.rs + daemon.rs swallow-sites hold
  until the queue verdict lands.
- **scheduler execution steps 1-2** (scope rows + readiness, resource-aware
  scheduler plan section 4) — written against jobq's `_job` table and claim
  path. Under an adopted queue these re-target the crate's job metadata
  (e.g. effectum job payload/weight) or a sidecar `_job_scope` table keyed by
  the crate's job id. The design (declared read/write sets, conflict-gated
  admission) survives; the substrate tables change. Hold dispatch until
  verdict.
- **class 18 residual: req_id mid-tick cancellation** (JobRow.req_id always
  None) — moot if the adopted queue has first-class cancellation; the
  residual becomes "wire CLI req_id to <crate> cancel API".
- **jobq lease sweep / orphan recovery** (mod.rs:47-49, borrowed from apalis
  reenqueue_orphaned and noted stronger than effectum's startup-only
  recovery) — moot on adoption; recovery semantics come from the crate.
  The "why after SIGKILL" requirement transfers as an acceptance criterion
  on the crate's on-disk job state.

### 4b. HTTP/RPC adoption moots / reshapes

- **erase public no-daemon split** (queued directive: one server code path,
  `--no-daemon` internal-only) — not mooted; it becomes the adoption commit
  itself. Executing it twice (once bespoke, once on the adopted framework)
  is the waste to avoid: hold until framework verdict, then land as one arc.
- **decomposition step 10** (absorb daemon_read/daemon_http/daemon_shell under
  src/daemon/, plans/2026-07-18-decomposition-normalization.md section 7,
  open decision 2) — held on the scheduler plan already; now also held on
  this plan. If daemon_http's hand-rolled serving is replaced wholesale,
  moving it first is moving a file scheduled for deletion.
- **db-seam batch A1 (daemon_read.rs 21/3/0)** — NOT mooted: daemon_read is
  query serving over the engine db, which survives any framework swap.
  Proceeds as planned.

### 4c. Supervision adoption moots / reshapes

- **decomposition step 6** (daemon.rs → src/daemon/{mod,root,home,budget,
  dispatch,client}, held on scheduler plan per section 8) — now held on this
  plan too. If launchd/systemd own daemonization, respawn, and log routing,
  the {root,home} split targets shrink and `budget` may reduce to
  per-process QoS/IOPOL only (plist keys take over the rest). Splitting
  first fragments code that adoption deletes.
- **rails pair in flight (stale-binary warn)** — reshaped, still wanted:
  under launchd the check compares repo build mtime against the plist's
  program path target; keep the rail, re-point the probe at adoption time.
  Not a reason to pause that agent (the warn logic is seam-independent).
- **daemon respawn/self-fork code** (bespoke start/stop, pidfile, respawn)
  — direct deletion target under service-manager adoption; the "zero
  respawns" receipt in the blocked redeploy item becomes a launchd
  ThrottleInterval/KeepAlive observation instead.

### 4d. Explicitly NOT mooted

- **eprintln→tracing conversion** (in flight) — complementary; tracing is
  the spine every adopted crate hooks into. Proceeds.
- **storage-diet step 2, cold-chunk extension, eaten-diag classes** (in
  flight) — engine-core work, untouched by infra adoption. Proceed.
- **db-seam batches A1-A3, A5-A8, B1** — engine/app SQL, not infra. Proceed.
- **invlog/why/verdict → tracing subscribers** (obs plan) — already law;
  adoption here only adds emitters, the subscriber migration is its own arc.

## 5. Verdicts and migration sequencing

### 5.1 Summary verdicts

| Subsystem | Adopt | Runner-up | Bespoke residue that stays |
|---|---|---|---|
| Job queue | apalis 1.0-rc + apalis-sqlite (risks: rc churn, libsqlite3-sys pin, running-task cancel, bus factor 1) | effectum fork | Admission layer above the queue: scope rows, conflict gating, write-volume budget (scheduler plan R2/R3 — no crate ships it) |
| HTTP + UDS RPC | axum 0.8: one Router, two thin listeners (TCP + UDS), TraceLayer, SSE for watch | poem (`Listener::combine`) | Handler bodies, core bridge (channel/spawn_blocking into sync tick loop) |
| Supervision | launchd LaunchAgent / systemd user unit via service-manager 0.11.0 (hand-authored plist/unit through `contents`) + fd-lock + sd-notify | in-process supervisor loop (fallback only) | Thread caps, macOS CPU-percent budgeting (no OS mechanism), stale-binary detection, `dl daemon why` trail, fd-lock witness |

Cross-cutting: every adopted layer hooks the existing `tracing` spine
(apalis tracing-native, axum TraceLayer, journald/launchd log routing),
consistent with the obs plan's subscriber migration.

### 5.2 Decision gates before execution

1. **libsqlite3-sys pin** (1.6 q1) — the single hardest blocker; resolves
   the apalis-vs-effectum-fork choice. Prototype first.
   **RESOLVED 2026-07-18 (prototyped)**: repo pin is rusqlite 0.31 (not
   0.40.1 as section 1 states — correct that on read). rusqlite 0.31
   (libsqlite3-sys ^0.28) vs apalis-sqlite rc.8 → sqlx 0.8.6
   (libsqlite3-sys ^0.30.1) is a links collision as predicted. Bumping to
   rusqlite 0.32.1 unifies the lockfile on a single libsqlite3-sys 0.30.1
   (verified via `cargo tree -i`), and dl on rusqlite 0.32 gets a clean
   `cargo check` + 570/570 `cargo test --lib`. Note apalis-sqlite's
   latest is rc.8 (rc.9 exists only for apalis/apalis-sql, and apalis-sql
   has no `sqlite` feature — the sqlite store is the separate
   apalis-sqlite crate). The apalis path is viable with a one-version
   rusqlite bump; effectum's fork rationale loses its pin argument.
2. **ProcessType=Background clamp** (3.6 q1) — decides whether first-run
   rebuild stays acceptable on efficiency cores; shapes the budget split.
   **RESOLVED 2026-07-18 (prototyped)**: temporary LaunchAgent A/B, 5s fixed
   CPU spinner (arm64, macOS 14.6.1, `RunAtLoad` + explicit `kickstart -p`
   since the automation session's `gui/$UID` domain came up in
   on-demand-only mode and never auto-fired `RunAtLoad`), bootout + plist
   delete after each trial. Two trials: no-`ProcessType` baseline
   2,505,048,064 / 2,500,853,760 iterations vs `ProcessType=Background`
   1,145,044,992 / 1,127,219,200 iterations — Background sustained
   **~45%** of baseline throughput both times (a ~2.2x wall-clock
   slowdown), consistent with the efficiency-core-only clamp on Apple
   Silicon. Verdict: **Nice+LowPriorityIO, not ProcessType=Background.**
   The in-process CPU-percent governor (`budget::start_governor`, kept
   per 3.5 point 3) already bounds total consumption to a fixed ceiling —
   that is the mechanism doing "nothing seizes the machine" duty here.
   `ProcessType=Background`'s hard efficiency-core clamp stacks on top of
   that ceiling for no added safety and a measured ~2.2x cost to a
   legitimate first-run rebuild. `Nice`+`LowPriorityIO` mirror the deleted
   in-process `nice()`/`setiopolicy_np()` calls at the plist level without
   the extra clamp.
3. **rc pin tolerance** — user call: pin apalis rc.9 now vs wait for 1.0.
   **DECIDED 2026-07-18 (user)**: pin the rc (`=` pins + committed lockfile);
   absorb the rc→1.0 diff whenever upstream ships it.

### 5.3 Sequencing (each step shippable, ordered by independence)

1. **Supervision** first — smallest blast radius, no schema contact:
   service-manager wiring, plist/unit authoring, delete self-fork +
   respawn + macOS nice/IOPOL calls, fd-lock single-instance, re-point
   stale-binary rail. Unblocks decomposition step 6 with a smaller
   daemon.rs.
2. **HTTP/UDS** second — axum shell, one Router, erase the public
   no-daemon split in the same arc (queued directive lands here, once).
   Unblocks decomposition step 10.
3. **Queue** last — largest surface; gated on 5.2 q1 prototype. Lands with
   the req_id cancellation wiring (class 18 residual) and re-targets
   scheduler steps 1-2 onto the adopted store per section 4a.

### 5.4 Held work released by this plan's verdicts

- db-seam A4: effect.rs proceeds now; jobq/mod.rs + daemon.rs swallow
  sites hold until step 3 verdict executes (4a).
- decomposition steps 6/10: released after sequencing steps 1/2
  respectively (4b/4c).
- scheduler steps 1-2: re-cut against the adopted queue store after step 3.
