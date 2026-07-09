# Daemon live activity in `status` — plan

**Goal:** at any moment, `dl daemon status` (and the `ping` RPC) reports *what the
current loop is doing and why* — which phase, which file/rel/program — so you can
see "wtf is happening" instead of just `tick_count`/`settled`.

Written 2026-07-09. Context: the daemon is one warm engine behind a single
`eng: Mutex` (`daemon.rs:621`); a tick holds that lock for its whole duration.
`ping` already avoids the eng lock (reads atomics + the small `program_files`
mutex, `daemon.rs:1144`), so it stays responsive mid-tick. The activity slot must
also stay OFF the eng lock, or status blocks exactly when you most want it.

## Type signatures first

```rust
// New module: src/engine/activity.rs (or src/activity.rs — engine-agnostic).

/// A cheap, lock-light snapshot of what the engine is doing right now. The tick
/// swaps it at phase boundaries; ping reads it. NOT behind the engine Mutex.
pub struct Activity {
    pub phase: Phase,        // coarse step (parse/extract/reconcile/derived/effects/idle)
    pub detail: String,      // the file / rel / stratum / program in flight
    pub program: String,     // which input program set (display string)
    pub tick: u64,           // tick number this activity belongs to
    pub since: Instant,      // when this phase started (→ elapsed_ms in ping)
}

#[derive(Copy, Clone)]
pub enum Phase {
    Idle, ColdTick, Declare, Reconcile, ParseExtract, Derived, Operators,
    Effects, Query, Settling,
}
impl Phase { pub fn as_str(&self) -> &'static str { /* "idle" | "extract" | ... */ } }

// Process-global, one daemon per process. Mutex held only for the microsecond of
// the swap — never across a phase — so a reader never waits on real work.
static ACTIVITY: Lazy<Mutex<Activity>> = Lazy::new(|| Mutex::new(Activity::idle()));

/// Set the current phase + detail. Called from the tick at each boundary.
pub fn set(phase: Phase, detail: impl Into<String>);
/// Set only the detail within the current phase (per-file / per-rel updates).
pub fn detail(detail: impl Into<String>);
/// Mark the tick number + program at tick start; Phase::Idle at tick end.
pub fn begin_tick(tick: u64, program: &str);
pub fn end_tick();
/// Read a snapshot for ping (clones the small struct under the brief lock).
pub fn snapshot() -> ActivitySnapshot;   // { phase, detail, program, tick, elapsed_ms }
```

Rationale for a process-global over an `Arc` threaded through `Engine`: the tick
is deep (reconcile → per-file extract families → derived strata → operators →
effect drain); threading a handle to every call site is the invasive version.
One `static` written by the engine and read by ping is O(1) plumbing and matches
the existing `CMD_COUNT`/`stmt_ms` telemetry pattern (already process-global).

## Pseudo-code body (instrumentation points)

Boundaries live in `src/engine/tick.rs::tick_report` and the phase fns it calls:

```
tick_report(prog, quiet):
  activity::begin_tick(self.tick_number, &self.program_display)   // or pass display in
  activity::set(Declare, "")           ; declare_all / ensure_meta
  activity::set(Reconcile, "")         ; reconcile_sources(...)
      # inside reconcile / extract.rs cached_facts loop, per file:
      activity::detail(format!("{family}: {path}"))   # e.g. "extract:call: src/foo.rs"
  activity::set(Derived, "")           ; rebuild_derived(...)
      # inside rebuild_derived stratum loop:
      activity::detail(format!("stratum {i}/{n}: {rel}"))
  activity::set(Operators, "")         ; scc / node2vec / closure evals
  activity::set(Effects, "")           ; drain_effects / drain_streams (daemon side)
      activity::detail(format!("{kind} {head_rel}"))   # e.g. "sh! fetch_endpoint"
  activity::set(Query, "")             ; ? queries
  activity::end_tick()                 ; Phase::Idle
```

Cold tick (`daemon.rs:596`) wraps its `eng.tick` in `activity::set(ColdTick, display)`.
The watcher/poll loops set `Effects`/`Reconcile` around their drives.

Minimum viable set of markers (do these first, they answer 90% of "wtf"):
1. `ColdTick` around the startup tick.
2. `ParseExtract` + per-file `detail` in `extract.rs` `cached_facts` (the loop
   that dominates cold time — `refresh_type_rels`/`refresh_call_rels`/
   `refresh_dataflow_rels`, extract.rs:634/1168/1352).
3. `Derived` + per-stratum `detail` in `rebuild_derived` (mod.rs:4623).
4. `Effects` + per-effect `detail` in the drain (daemon side).

## Instance lifetimes / storage

- `ACTIVITY`: process-static, lives for the daemon process. One writer at a time
  in practice (ticks are serialized by the eng lock), so the Mutex is uncontended;
  readers (ping handlers, each on its own connection thread) take it briefly.
- No new field on `Daemon` or `Engine` required if global; if we prefer no global,
  add `Arc<Mutex<Activity>>` to `Engine` (set in `Engine::new`) and clone it into
  the daemon for ping — costs one field + one ctor arg.

## ping / status wire changes

`ping` handler (`daemon.rs:1144`) adds an `activity` object:

```json
"activity": { "phase": "extract", "detail": "call: src/engine/mod.rs",
              "program": ".dl/*.dl", "tick": 42, "elapsed_ms": 1830 }
```

`dl daemon status` (`src/cli/daemon.rs::print_status`) prints one line:

```
daemon: running  (root /Users/…/sprefa)
  build_id   0.6.21:…
  tick_count 42
  settled    false
  doing      extract call — src/engine/mod.rs   (1.8s, tick 42)
```

`Phase::Idle` prints `doing   idle` (settled) so a quiet daemon reads clean.

## Also fixes the startup blind spot (ties to bind-then-tick)

Today `status` says "not running" during the cold tick because the socket binds
AFTER the tick (`daemon.rs:596` tick, `:642` bind). Two independent options:
- **Cheap:** leave ordering; this plan makes a *warm* daemon self-describe. The
  cold-tick window still shows "not running" (no socket yet).
- **Full:** bind-then-tick — bind + spawn accept first, run the cold tick on a
  worker, add a `ready`/`warming` flag to `ping`. Then `dl daemon status` during
  cold start reads `doing cold-tick — <program> (3.2s)` instead of "not running".
  Cost: give up the "connectable ⇒ warm" contract; `ensure_daemon::wait_ready`
  must check the `ready` flag instead of treating any connect as ready.

Recommend: ship the activity slot first (self-contained), then the bind-then-tick
flip as a follow-up if the cold-start window matters.

## Overhead

A `Mutex<Activity>` swap per file / per stratum is ~ns and off the hot SQL path;
negligible vs parse/extract. Guard the per-file `detail()` behind the same
`quiet`/`daemon`-only check the `[tick]` log uses if we want zero cost in
one-shot `--no-daemon` runs (the slot is only read by ping, so a one-shot never
reads it — updating it is harmless but skippable).

## Test

- Unit: `activity::set` then `snapshot()` round-trips phase/detail/elapsed.
- e2e (`tests/it/daemon.rs`): spawn a daemon on a fixture with a slow-ish extract
  (or a `sh` effect + `DL_POLL_SECS`), fire `ping` during the drive, assert the
  `activity.phase` is non-idle and `detail` names a real file/rel; assert it
  returns to `idle` after settle.

## Not doing (scope guard)

- No per-SQL-statement live trace (that's `DL_PROFILE`/`stmt_ms` after the fact).
- No progress percentage (files X/Y is enough; a percent needs a pre-count pass).
- No streaming/subscribe of activity (ping-poll is enough; `subscribe` already
  exists if we later want push).
```
