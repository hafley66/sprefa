# G4v2 — port the EXISTING labkit onto sprefa-store + add a hermetic runner

## HARD RULES (read twice)

1. **The harness ALREADY EXISTS.** `v6/labkit/` has 14 source modules, the
   `Experiment` trait, the `Harness` Big-O scale-sweep, the grand table, plan
   snapshots, and 11 engines. You **edit it in place**. You do **NOT** write a
   new minimal harness. You do **NOT** delete any engine. You do **NOT** invent
   a fresh `tick() -> Answer` trait. A prior attempt rebuilt from scratch
   because its worktree was empty; this one is not — the code is right there.
   If you find yourself creating a new trait or a 3-engine driver, STOP: you are
   doing the wrong thing.

2. **Output discipline.** No narration. No progress chatter. No status prose.
   Do the edits, run the validation, write ONE terse result file. Nothing else.

## What is here (confirm before touching)
- `src/lib.rs` — `Experiment` trait, `Harness`, `Workload`, digest/oracle glue.
- 11 engines: RamZset, RamReach, SqliteReach, CascadeZset, Reconciler,
  SqlReconciler, SalsaReconciler, SqliteTemporal, DdReach, DdBfs (+ SalsaRows).
- 7 modules use `rusqlite`: cascade.rs, reconcile.rs, sqlite_exp.rs,
  reach_dred.rs, reach_exp.rs, sqlmem.rs, reach_inc.rs.

## Part A — SQLite in ONE crate (remove rusqlite)
- Drop `rusqlite` from `v6/labkit/Cargo.toml`. Add:
  `sprefa-store = { path = "../sprefa-store" }`, `sea-orm` (match the store's
  features: `default-features=false, features=["sqlx-sqlite","runtime-tokio-rustls"]`),
  `tokio = { version="1", features=["rt"] }`.
- Port each of the 7 modules: replace `rusqlite::Connection` with a sea-orm
  `DatabaseConnection`. Get it via
  `sprefa_store::relstore::RelStore::attach(sea_orm::Database::connect(opts).await?)`
  (that runs the store's OPEN_PRAGMAS) and `store.conn()` for the raw
  `&DatabaseConnection`. Run each engine's bespoke SQL through
  `conn.execute_unprepared(sql)` and `conn.query_all(Statement::from_string(...))`.
  Where an engine's operation IS the counting cascade, prefer RelStore typed
  ops: `add_rows`, `add_deps`, `retract`, `retract_dred`, `retract_dred_cte`,
  `alive_keys`, `alive`.
- Async/sync bridge: `Experiment::tick` is sync. Each SQLite engine holds ONE
  `tokio::runtime::Builder::new_current_thread()` runtime for its lifetime and
  `runtime.block_on(...)` the async store calls. NEVER build a runtime per tick.
- `libsqlite3-sys` is allowed ONLY for the `sqlite3_memory_highwater` probe in
  gun.rs / sqlmem.rs. rusqlite must be GONE (`rg -l rusqlite v6/labkit/` empty).

## Part B — hermetic per-process runner
- Add `src/bin/0_unified.rs`. It runs each (engine, workload, scale) cell in a
  SEPARATE OS process by re-execing itself with a `--child` arg. Per child:
  install `DL_MEMCAP_MB` (memcap::cap_address_space_mb), build engine state
  OUTSIDE the timer, drop staging before `tick`, reset rust peak + sqlite
  highwater, then time the tick. Answer digest is checked against the engine's
  independent oracle (the `Workload`'s expected_digest already provides this).
- A reference for the process/memcap/bridge MECHANICS exists on branch
  `58d21c32` at `v6/labkit/src/bin/0_unified.rs`. Reuse its `run_child` /
  re-exec / getrusage / highwater plumbing, but wire it over the EXISTING
  `Experiment` trait and ALL engines above — NOT a new 3-engine trait.
- Keep the existing `Harness` (in-process Big-O sweep, grand table, plan
  snapshots) COMPILING and working. It is the analytic view; `0_unified` is the
  memory-honest hermetic view. Both build.

## Validate (paste real output into the result file)
- `cd v6/labkit && cargo build` — green.
- `cargo build --features with-dd,with-salsa` — green.
- `cargo run --bin 0_unified` — writes `UNIFIED-REPORT.md`; every cell digest
  matches its oracle (the naive-counting-on-cycles phantom is the ONE allowed
  mismatch, documented as such, because the pinned design scopes fixpoint by
  SCC — do not "fix" it).
- `rg -l rusqlite v6/labkit/` — EMPTY.

## Commit
- Leave everything STAGED: `git add -A v6/labkit`. Do NOT commit — the
  pre-commit hook execs `dl` which is not on PATH and will fail. Do NOT bypass
  the hook. The coordinator commits.

## Result file
Write `v6/labkit/EXPERIMENT-G4V2-RESULT.md`: which modules ported, any engine
that could not be ported and why, and the raw validation output. Terse. No
prose beyond that.
