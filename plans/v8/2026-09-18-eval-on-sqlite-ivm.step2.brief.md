# eval-on-sqlite-ivm step 2: one `open()`, one `sql()` span, the contract test

Parent brief: `plans/v8/2026-09-18-eval-on-sqlite-ivm.brief.md` on
`origin/main` (`06a02da2f`). Read its "Machine laws", "Connection contract",
"Tracer at every SQL boundary", "Laws inline", and "Addendum 1" first; they
bind this lane and this brief does not repeat the addendum table. Branch
`chore/sqlite-open-contract` from `origin/main`. One PR. Report to the driver
lane `plan-eval-on-sqlite-ivm`.

## First action, before anything else

```bash
ln -s ~/projects/sqlite_ivm sqlite_ivm; ln -s ~/projects/hafley-rs hafley-rs
```

Both are gitignored. Without them `_16`, `_18`, `_20` fail for environmental
reasons. If `~/projects/sqlite_ivm/target/release/libsqlite_ivm.dylib` does
not exist, stop and report; do not build sqlite_ivm yourself.

## Connection sites today (every one reroutes through `open()`)

| site | today |
|---|---|
| `src/_9_runtime/_1_sqlite.rs:39` | `Connection::open(path)?` in `SqliteRowStore` |
| `src/_9_runtime/_1_sqlite.rs:130`, `:461`, `:555`, `:637`, `:643` | `execute_batch` DDL, `BEGIN IMMEDIATE`, `COMMIT`, `ROLLBACK` |
| `tests/_18_sqlite_emit.rs:29-49` | `extension()` locates `libsqlite_ivm` (env `SQLITE_IVM_LIB`, `$CARGO_TARGET_DIR/release`, `sqlite_ivm/target/release`) |
| `tests/_18_sqlite_emit.rs:310-318` | open, `load_extension`, the two sqlite_ivm pragmas, DDL |
| `tests/_13_store.rs:112`, `:136`, `:155` | `rusqlite::Connection::open(db)` read-back |

`Cargo.toml:16` has `rusqlite` without `load_extension`; `:28` (dev-deps)
has it. Moving the feature to `:16` is inside the owned deps block.

## Build

1. `pub fn open(path: &Path) -> rusqlite::Result<Connection>` in
   `src/_9_runtime/_1_sqlite.rs`. Runs the eight contract pragmas in the
   parent brief's order (`page_size` first, before any table), then
   `load_extension` via the locator lifted from `tests/_18_sqlite_emit.rs:29-49`
   (runtime fallback: `env!("CARGO_MANIFEST_DIR")/sqlite_ivm/target/release`),
   then nothing else yet (scalar functions arrive in step 4). `:memory:`
   is accepted; `journal_mode` reads back `memory` there, so the pragma
   assertions run on a file db.
2. `pub fn sql<T>(connection: &Connection, name: &'static str, rows: impl FnOnce(&Connection) -> rusqlite::Result<(T, usize)>) -> rusqlite::Result<T>`,
   or the closest shape that compiles cleanly: one
   `tracing::info_span!("sql", name, rows = tracing::field::Empty, ms = tracing::field::Empty)`
   entered around the call, `rows` and `ms` recorded after. A multi-row
   `INSERT` of a seed set is ONE call with its row count. No `trace_v2`.
3. Every `execute_batch`, `prepare`, `execute`, `query` in
   `src/_9_runtime/_1_sqlite.rs` runs inside `sql`. `grep -n 'execute\|prepare\|query' src/_9_runtime/_1_sqlite.rs`
   before and after, pasted, every hit accounted for.
4. Tests `_13_store` and `_18_sqlite_emit` open through `dl8::...::open`
   (the lifted locator deleted from `_18`).
5. `tests/_25_sqlite_contract.rs`: the eight rows of Addendum 1, one test
   function per row, through `open()` and `sql()`, file db under the test's
   temp dir (`tempfile` if already a dev-dep, else `std::env::temp_dir()` plus
   a unique name; check `Cargo.toml` first). Span ms and counts come from a
   `tracing_subscriber` layer the test installs (check `tests/_9_tracing.rs`
   for the existing collection pattern and reuse it). Ratios and COUNTs only,
   never a bare wall-clock threshold except the one Addendum 1 names (batched
   10k under 50 ms). Each test function finishes in under 10 s in debug; if
   the 100k or 64 MiB rows exceed that, report the measured wall and stop,
   do not shrink the case silently.
6. `justfile` `trace-query` gains a `rust_ms` column (phase ms minus the sum
   of its child `sql` ms). Receipt: `just trace-up && just trace compile
   fixtures/openapi/todo.dl7 && just trace-down && just trace-query`, output
   pasted. `trace-down` is mandatory even on failure; nothing left running.

7. `tests/_26_bounded_loops.rs`, the scanner for the bounded-loop law
   (`CLAUDE.md:53-58` on `origin/main`, quoted in "Laws"). It walks
   `src/**/*.rs`, finds every `loop {`, `while `, and every fn that calls
   itself by name, and for each hit looks for a budget line within the
   enclosing block: a `for _ in 0..` bound, a depth counter compared to a
   named constant, or a repeat check, plus a named diagnostic on the cap.
   Today `grep -rn 'loop {' src` finds 15 sites, most in forbidden paths
   (`_0_read`, `_2_lower`, `_1_macrotime`, `_4_comptime`, `_5_reify`,
   `_6_eval`). Do NOT edit them. The test carries an explicit list
   `UNBUDGETED: &[(&str, &str)]` of `(path, trimmed line text)` for every
   site that lacks a budget today (line text, not line number, so edits
   elsewhere do not break it). It fails on: a site with no budget that is
   not in the list, and a listed site that no longer exists or now has a
   budget (the list only shrinks). Any loop you add in owned files carries
   its budget from the start. PR body pastes the list with its length.

## Receipts in the PR body

1. The eight Addendum 1 rows with the numbers from one run.
2. The `just trace-query` output with `phase`, `sql`, `rust_ms` columns.
3. `timeout 600 cargo test --no-fail-fast` once; failures compared to
   `.github/CI-KNOWN-RED.md` "dl8 cargo battery". A red NOT in that file is
   yours until you prove otherwise by running the same leg on `origin/main`
   in the same worktree (`git stash` is banned; use `git worktree add
   --detach /tmp/main-check origin/main`, remove it after).
4. `_22_book::chapter_executors` alone, three times, walls pasted.
5. Oracles byte-identical: `git diff --stat origin/main...HEAD -- oracle/` empty.
6. `ps -eo pid,pcpu,rss,args | grep -E 'dl8|otel|duckdb'` before and after.

## Ownership

Owned: `src/_9_runtime/_1_sqlite.rs`, `src/_9_runtime/mod.rs` (re-export
only), `tests/_13_store.rs`, `tests/_18_sqlite_emit.rs`,
`tests/_25_sqlite_contract.rs`, `tests/_26_bounded_loops.rs`, `Cargo.toml` deps and dev-deps blocks,
`justfile` `trace-query` recipe only, `docs/failure-modes.md`.
Forbidden: every other path, including `src/_6_eval/` (step 4 owns it),
`src/_5_reify/`, `oracle/`, `book/`, `sqlite_ivm/`. A needed sqlite_ivm
change is a stop-and-report with the exact SQL it rejected.

## Laws

Read `.claude/skills/sqlite-costs/SKILL.md` and
`.claude/skills/sql-relational-design/SKILL.md` before writing DDL in the
contract test. No `eprintln!` in `src/**`. Comments state only what the code
cannot show. Descriptive names. No em dashes. Banned: provenance, substrate,
load-bearing, regime, "ground truth", "rel" in prose. `timeout 10` on every
`dl8` run; a timeout is the result. One cargo at a time, never override jobs.
No subagents. No background processes left running.

Bounded loops (user law, `CLAUDE.md:53-58`): a `loop {}`, `while`, or
recursive fn in `src/**` carries an explicit budget (`for _ in 0..LIMIT`, a
depth counter, a row-set repeat check) and stops with a named diagnostic when
the budget is hit. The bound is a constant with a comment saying what it
protects. A fixpoint with no budget is a blocking defect.

```bash
boop beep --no-wait --as chore-sqlite-open-contract plan-eval-on-sqlite-ivm "step 2: PR #<n>, contract 8/8, battery <pass>/<total>, sql spans <count> in todo.dl7 trace, rust_ms <ms>, red: <list or none>"
```
