# Settle / quiescence detection

## Goal

Guarantee a program "ran and settled at least once": drive ticks until every
cascade (`@next`, `@async`/`sh*` drains, demand hops, `repo`-sink pulls) has
converged, then stop — ignoring recurrent no-change timer ticks (`every`/`clock`/
`@stream`). Fail loudly, not hang, when a program cannot settle.

Two surfaces:
- **`dl --settle prog.dl`** — one-shot, in-process, WITH the effect runtime
  wired in (today it is daemon-only). The primary deliverable.
- **daemon `await_quiescent` RPC** — read side over the already-running poll
  loop. Follow-on.

Research grounding: research/subagent report this session; line refs verified
below against `src/engine/tick.rs`, `src/engine/mod.rs`, `src/effect.rs`,
`src/daemon.rs`, `src/lib.rs`.

## Current state (verified)

- `Engine::tick(&mut self, prog, quiet) -> Result<()>` (`tick.rs:15`) computes a
  local `changed: bool` (`tick.rs:213`) and **discards** it — only printed to
  stderr (`tick.rs:388`). No caller can read whether the tick moved anything.
- `drain_effects` / `drain_streams` are called from **exactly one place**:
  `poll_tick` (`daemon.rs:370-371`). No non-daemon path drains effects, so
  `--check` / one-shot (`run_check_inproc` `lib.rs:318`, ticks once at `:332`;
  `run_file_inproc` `:178`, ticks once at `:196`) leave `@async`/`sh*` requests
  stuck `queued` in `pending_effect` forever.
- `load_carry(&self, rel, meta, tx) -> Result<bool>` (`mod.rs:3985`) is
  **destructive**: it deletes + reloads the live table as its only mode. No way
  to peek "would the carry differ" without mutating.
- `shell_kinds(prog) -> HashMap<String, ShellKind>` (`effect.rs:260`) already
  maps effect kind → `ShellKind`; `ShellKind::Stream` is the never-settles
  marker (`effect.rs:544` skips it in `drain_effects`).
- Daemon has `tick_count: AtomicU64` (`daemon.rs:202`), `poll_tick` (`:336`)
  already loops `tick_full; drain_effects; drain_streams; if n>0 { tick_full }`.
- No `--settle`/`--wait`/`--once` flag; no `settle`/`quiescent` string anywhere
  in `src/`.

## Multi-tick cascade depths (why one tick is not enough)

| mechanism | ticks | in-flight signal |
|---|---|---|
| `@next` carry | 1 | `carry_<rel>` at `tx+1` differs from live rel |
| `@async` effect | ≥2 | `pending_effect.state IN ('queued','running')` |
| `repo`-sink pull | ≥2 | new slug in `repo` builtin next tick (`tick.rs:445`) |
| `scip_want` demand | ~3 | `scip_want` rows vs `scip_def/ref` digest |
| `@stream` / `every` / `clock` | ∞ by design | steady-state, MUST be excluded from settled |

## Phase 1 — `tick` reports what moved

Change `tick` (and `tick_paths`, for the daemon/`--changed` path) to return a
report instead of `()`. Keep the internal logic identical; surface the bits it
already computes.

```rust
// src/engine/tick.rs
#[derive(Default, Clone, Debug)]
pub struct TickReport {
    pub changed: bool,          // = the existing local `changed`
    pub derived_moved: bool,    // derived:program digest moved
    pub changed_rels: Vec<String>,   // = changed_source_rels drained to a Vec
    pub staged_next: bool,      // any carry_<rel> at tx+1 differs from live (Phase 2)
    pub inflight_effects: usize,// pending_effect queued|running, non-stream (Phase 3)
}

pub fn tick(&mut self, prog: &Program, quiet: bool) -> Result<TickReport> { ... Ok(report) }
```

- Callers today `eng.tick(&prog, q)?;` ignore the value — a returned struct is
  source-compatible with `?;` (the `Result` unwraps, the struct drops). Only new
  callers read it. Confirm every existing `eng.tick(` site compiles unchanged
  (grep: `lib.rs` ×6, `daemon.rs` tick wrappers, tests).
- `tick_full` / `tick_paths` wrappers (`daemon.rs:220-245`) forward the report
  or map to `()` as their callers need.

## Phase 2 — non-destructive carry peek

`load_carry` mutates. Add a read-only twin so the settle predicate can ask
"will next tick's carry change the live rel" without applying it.

```rust
// src/engine/mod.rs, beside load_carry
fn carry_differs(&self, rel: &str, meta: &RelMeta, tx: i64) -> Result<bool> {
    // content-digest of carry_<rel> rows at `tx` vs live rel `rel`, no writes.
    // reuse rel_content_digest (mod.rs:2569) over the staged rows.
}
```

- `tick` sets `report.staged_next = any(next_rels, |r| carry_differs(r, ...))`
  right after `rebuild_next` (`tick.rs:470`), reading the carry it just staged
  at `cur_tx+1`.

## Phase 3 — stream-aware in-flight count

`is_settled` must not count `@stream` rows (they stay `running` forever by
design). `shell_kinds` already classifies; expose the count.

```rust
// src/effect.rs (or engine)
pub fn inflight_nonstream(&self, prog: &Program) -> Result<usize> {
    let kinds = shell_kinds(prog);           // effect.rs:260, already exists
    // SELECT kind FROM pending_effect WHERE state IN ('queued','running')
    //   count rows whose kind is NOT ShellKind::Stream
}
```

- `tick` sets `report.inflight_effects = self.inflight_nonstream(prog)?` near the
  end (after `rebuild_async`, `tick.rs:476`).

## Phase 4 — the settled predicate

Settled = one tick produced no non-timer motion AND nothing is pending.

```rust
// src/engine/mod.rs (method on Engine, or a free fn over a TickReport)
fn is_settled(report: &TickReport) -> bool {
    !report.derived_moved
        && report.changed_rels.iter().all(is_timer_rel)  // every/clock only
        && !report.staged_next
        && report.inflight_effects == 0
}
fn is_timer_rel(rel: &str) -> bool { rel == "every" || rel == "clock" }
```

- `changed` alone is too coarse (a clock boundary sets it). Gate on
  `changed_rels` minus timer rels + the three pending signals.
- Demand hops (`scip_want`, `repo`-sink) have no stored in-flight bit; they are
  caught transitively — they mutate a source/derived digest, so `derived_moved`
  or a non-timer `changed_rels` entry stays true until they quiesce. The loop
  re-ticks until that stops.

## Phase 5 — `dl --settle prog.dl` (one-shot, drives effects)

New in-process mode: loop `tick + drain_effects + drain_streams` until settled
or a tick budget, then run `?` queries once (like `run_file_inproc`).

```rust
// src/main.rs Cli
#[arg(long, help_heading = "Run modes")]
settle: bool,                       // run until quiescent, then print ? results
#[arg(long, help_heading = "Run modes")]
settle_max: Option<usize>,          // tick budget (default 200)

// src/lib.rs
fn run_settle_inproc(programs, db_path, root, budget) -> Result<()> {
    // build engine + program like run_file_inproc (:178)
    // build ShellEffectExec like poll_tick (daemon.rs:355-367): shell_templates
    //   + effect_cmd overlay + async_effect_arity; skip drains if arity empty.
    // loop:
    //   let r = eng.tick(&prog, true)?;
    //   let n = eng.drain_effects(&prog,&exec)? + eng.drain_streams(&prog,&exec)?;
    //   if is_settled(&r) && n == 0 { break }          // converged
    //   // no-progress guard: track the SET of still-disagreeing signals
    //   //   (changed_rels∖timer ∪ inflight ids ∪ staged_next). If |set| does
    //   //   not shrink for STALL=10 consecutive ticks, bail loudly naming the
    //   //   oscillating rels/effects (mirror mod.rs:4258 iters>100_000 bail).
    //   if iter >= budget { bail!("did not settle in {budget} ticks: {still}") }
    // then run ? queries (tick.rs:429 does this inside tick when !prime_tick;
    //   here run_query over prog.items once at the end).
}
```

- **The real missing capability is the drain wiring**, not the flag: this is the
  first non-daemon caller of `drain_effects`/`drain_streams`. Reuse
  `ShellEffectExec` construction verbatim from `poll_tick`.
- **No-progress bail** distinguishes "still cascading" (set shrinking) from
  "cannot settle" (a `@next` counter, an always-changing poll, an `sh!` whose
  idempotency key never stabilizes). Name the offenders in the error.
- **Timer programs** (`every`/`clock`/`@stream`): `is_settled` already ignores
  timer rels; a pure `@stream` request is excluded from `inflight_effects`. So a
  poll-head.dl-shaped program settles at its first quiet point instead of
  hanging. Document that `--settle` returns after ONE converged state, it does
  not keep the daemon-style loop alive.

## Phase 6 — daemon `await_quiescent` RPC (follow-on)

The poll loop already drives toward settlement; add the read side.

```rust
// src/daemon.rs
pub settled: AtomicBool,   // beside tick_count (:202); set after each poll_tick
// in poll_tick (:381) and tick_full/tick_paths wrappers: compute is_settled
//   from the TickReport (Phase 1) and store.
// handle_request: extend "ping" (:1111) with "settled": d.settled.load(...),
//   OR add a blocking "await_quiescent" {timeout_ms} that parks the socket
//   (mirror `subscribe` kept-open pattern, :210) until settled flips or timeout.
```

- Caveat unique to B: a daemon serving a clock/stream program is never strictly
  settled. `await_quiescent` must mean "no non-timer motion since the last clock
  boundary" — same semantics as Phase 4, evaluated per poll.

## Files

| file | change |
|---|---|
| `src/engine/tick.rs` | `TickReport` type; `tick`/`tick_paths` return it; populate the 5 fields from existing locals + Phases 2/3 |
| `src/engine/mod.rs` | `carry_differs` (P2); `is_settled` + `is_timer_rel` (P4) |
| `src/effect.rs` | `inflight_nonstream` (P3) |
| `src/lib.rs` | `run_settle_inproc` (P5); dispatch from `--settle` |
| `src/main.rs` | `--settle` / `--settle-max` flags + dispatch |
| `src/daemon.rs` | (P6) `settled: AtomicBool`, populate in poll/tick wrappers, `ping`/`await_quiescent` |
| `docs/daemon.md` | document `--settle` in the flag table + RPC table |

## Verification

New `tests/it/settle.rs`:

1. **converges-effectful**: program with one `@async`/`sh` request whose response
   feeds a derived rel; `dl --settle` (in-process, `DL_NO_DAEMON=1`) exits 0 and
   the `?` result includes the drained response — proves drains run off-daemon.
2. **demand-hop settles**: a `scip_want`/`repo`-sink-shaped fixture converges in
   ≤ its known depth, `--settle` returns with the demanded rows present.
3. **no-progress bail**: a `count(n+1) <- @next count(n).` counter fixture;
   `--settle --settle-max 20` exits non-zero with a message naming `count`, does
   NOT hang.
4. **timer program returns**: a `clock`/`every`-driven fixture settles at its
   first quiet point (no hang), `?` shows the first-boundary state.
5. (P6) **await_quiescent**: daemon e2e — spawn, issue an effectful load, block on
   `await_quiescent`, assert it returns true after the drain (not on timeout).

Unit: `is_settled` truth table over hand-built `TickReport`s (timer-only changed
= settled; non-timer changed = not; staged_next = not; inflight>0 = not).

## Sequencing

P1 → P2 → P3 → P4 → P5 (ships the CLI guarantee) → P6 (daemon read side).
P1–P4 are internal, no behavior change until P5 wires them. Land P5 with tests
1–4; P6 + test 5 can be a separate commit.

## Status (2026-07-06)

- **P1–P5 LANDED** (local, uncommitted). `tick_report` returns `TickReport`;
  `tick` is a thin `()` wrapper (zero churn at ~150 call sites). `carry_differs`
  (non-destructive peek, shares `digest_of_query` with `rel_content_digest`),
  `inflight_nonstream`, `TickReport::is_settled` + `is_timer_rel`. `dl --settle`
  / `--settle-max` drive tick+drain to a fixpoint in-process (the first
  non-daemon caller of `drain_effects`/`drain_streams`), with a STALL=10
  no-progress bail. `tests/it/settle.rs` (6): `is_settled` truth table, engine
  effect-inflight + carry-fixpoint reports, CLI converge/print, CLI real-`sh`
  drain off-daemon, CLI non-convergence bail. Suites lib 215/0/1, it 526/0/4.
  Docs: `docs/daemon.md` flag table.
- **P6 LANDED** (local, uncommitted). `Daemon.settled: AtomicBool` set by
  `tick_full` (via `tick_report`), cleared by `tick_paths` (a source change);
  the poll loop keeps ticking + draining toward it. `ping` gains a `settled`
  field; new `await_quiescent {timeout_ms}` RPC blocks the connection until the
  flag flips (or times out). CLI front-end `dl --await-settle [--await-settle-ms
  N]` (exit 0 settled / 3 timeout) + `daemon::await_quiescent` client. Test
  `tests/it/daemon.rs::await_settle_blocks_until_effect_drains` (daemon drives a
  real `sh` effect to a fixpoint; the client sees settled + the response landed).
