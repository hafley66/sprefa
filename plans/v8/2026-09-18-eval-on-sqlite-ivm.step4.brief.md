# eval-on-sqlite-ivm step 4: `IEvaluate` over rusqlite, behind a switch

Branch `feat/eval-sqlite-step4` from `origin/main`. One PR. Driver lane:
`plan-eval-on-sqlite-ivm`. The design is decided. Chris accepted F1b, F2a,
F3b, F4a and F5a in `plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md`, which
is on branch `docs/eval-on-sqlite-ivm-design` (PR #798) at
`/Users/chrishafley/projects/sprefa/.boop-worktrees/plan/eval-on-sqlite-ivm/plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md`.
Read it whole before writing code. Sections 2, 3 and 5 are your spec.

## You implement. You do not redesign.

Some decisions the design does not answer:

- a kernel mode not in section 2;
- a SQL shape sqlite_ivm rejects;
- a schema choice;
- a column, name, or file the design does not name.

When you hit one, send ONE line to the driver in the same turn, before you
build on it:

```bash
boop beep --no-wait --as feat-eval-sqlite-step4 plan-eval-on-sqlite-ivm "field decision: <what> | chose <X> | because <one clause> | site <path:line>"
```

Then continue with your choice. The driver forwards it to the coordinator. A
choice you do not report is a grading failure.

## Caps (Addendum 3 of the parent brief)

- Wall clock 30 minutes. At 25 minutes, commit and push whatever state you
  have, and open the PR with a body that lists what is missing. A partial PR
  graded green merges, and the remainder becomes the next lane's brief.
- Turn cap 150. Count your own tool calls and push at 140.
- Every `dl8` run is `RUST_LOG=dl8=info timeout 10 <dl8> ... 2> <log>`. On a
  timeout or a wrong answer, read the last 20 lines of the log and cite them.
  Never run it a second time "to see".
- Every `cargo test` is `timeout 600 cargo test ... > <log> 2>&1`, then read
  the log. One cargo at a time. Never override jobs.

## First action

```bash
ln -s ~/projects/sqlite_ivm sqlite_ivm; ln -s ~/projects/hafley-rs hafley-rs
ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '
```

## Slices, in order. Commit after each one.

### S1. Engine switch, `dl_*` functions, positive non-recursive rules

1. Change `Cargo.toml:16` `rusqlite` features: add `"functions"`.
2. Add `src/_6_eval/_8_functions.rs`: `pub fn register(connection: &Connection,
   arena: Arc<Mutex<Universe>>) -> rusqlite::Result<()>`. It registers every
   `dl_*` function from design section 2 with
   `FunctionFlags::SQLITE_UTF8 | SQLITE_DETERMINISTIC`. Each body calls the
   existing Rust in `src/_6_eval/_4_kernel.rs` (`cons_row`, `str_cons_row`,
   `edge_ref_row`, `intern_row`, `int_dot_add_row`, `Universe::cmp`) and
   returns NULL where that Rust returns `None`. The functions reuse the kernel
   code and do not reimplement it.
3. Add `src/_6_eval/_7_sqlite_eval.rs`:
   - `pub trait IEvaluate` with `declare`, `apply` and `read`, exactly as in
     design section 4.
   - `SqliteEvaluate`, which implements it over `crate::_9_runtime::sqlite::open`
     (`src/_9_runtime/_1_sqlite.rs`) and runs every statement inside `sql()`.
   - `pub fn evaluate_sqlite(u: &mut Universe, program: &Program, fx: &mut dyn
     FnMut(Trace)) -> Closure`. It moves the universe into the arena with
     `std::mem::take(u)`, opens `:memory:`, calls `declare`, `apply` (every
     seed) and `read` (every product), drops the connection, and moves the
     universe back.
   - One sqlite_ivm view per program (F3b). Its output is `(product INTEGER,
     c0 .. cK INTEGER)`, and source tables are one per base product with
     `UNIQUE` over every column.
4. Switch: add `static ENGINE: OnceLock<Engine>` in `src/_6_eval/_5_evaluate.rs`,
   read once from env `DL8_ENGINE` (`sqlite` or `rust`, default `rust`).
   `evaluate` (`_5_evaluate.rs:779`) branches on it and makes no other change.
   The Rust evaluator stays whole for this PR.
5. A rule the lowering cannot spell yet returns a diagnostic whose payload
   names it: `not_built_yet(<shape>)`. The shape names are the
   `Unsupported` variants at `src/_5_reify/_7_sqlite.rs:37-53`. Reuse that
   file's `Lowering` where it fits. You may move code out of it into
   `_7_sqlite_eval.rs`. Its `dl8 emit sqlite` output must stay byte-identical,
   checked by `tests/_18_sqlite_emit.rs` passing.
6. `tests/_27_eval_sqlite.rs` runs every `oracle/eval/*.json` through the real
   binary with `DL8_ENGINE=sqlite`, using the same comparison as
   `tests/_0_eval_oracle.rs:21-60`. It prints `sqlite oracles <pass>/<total>`
   and the name of each failing fixture. It asserts pass >= `FLOOR`, a
   constant set to your measured pass count. The floor only rises.

### S2. Recursion, negation, every kernel in section 2

Linear `WITH RECURSIVE` per SCC, with a `product` discriminator column for an
SCC of several products (design section 3). Negation is a correlated
`NOT EXISTS` over an earlier stratum's CTE. Add every mode row of section 2.
Nonlinear recursion (F4a): a `not_built_yet(nonlinear_recursion)` diagnostic,
plus one `tracing::info!(target: "dl8::eval", nonlinear_sites = N)` event per
`declare`. Raise `FLOOR`.

### S3. Folds, served goals, demand

Folds per section 2. `effect` output rows per section 3 "Served goals". F5a
demand CTE with `const DEMAND_DEPTH_LIMIT` (the comment states what it
protects) and a `demand_depth_exceeded` output row at the cap. Raise `FLOOR`.

### S4. `std/store.dl7`, emitted DDL and Rust

`build.rs` cannot call dl8's own lowering, because the crate is not built yet
when its build script runs. The bootstrap is therefore:

- `dl8 emit store std/store.dl7` writes `oracle/store/store.sql` and
  `oracle/store/store.rs`. Add a `Store` variant to `EmitTarget` in
  `src/bin/dl8.rs`.
- `src/_9_runtime/_4_store_schema.rs` is `include!("../../oracle/store/store.rs")`.
- `tests/_28_store_schema.rs` re-emits the store and diffs it against the
  frozen files.

The products and columns come from design section 1, and the Rust follows F1b
(structs plus `insert_*` / `select_*` / `delete_*`, each running inside
`sql()`). S4 does not reroute the existing `SqliteRowStore`. That is a later
step.

## Receipts in the PR body

1. `timeout 600 cargo test --test _0_eval_oracle --test _2_macrotime_oracle --test _8_compile_oracle --test _18_sqlite_emit --test _26_bounded_loops --test _27_eval_sqlite > /tmp/s4-targeted.log 2>&1`, with the result lines pasted. The default (Rust) path stays byte-identical, and `git diff --stat origin/main...HEAD -- oracle/eval oracle/macrotime oracle/compile` is empty.
2. `sqlite oracles <pass>/<total>` and the failing fixture names, each with its `not_built_yet(<shape>)` payload.
3. The `sql` span count for one fixture: `DL8_ENGINE=sqlite RUST_LOG=dl8=info timeout 10 target/debug/dl8 eval oracle/eval/0_transitive.json 2> /tmp/s4-sql.log; grep -c 'dl8::sql' /tmp/s4-sql.log`. Extract the program first, as `tests/_0_eval_oracle.rs:30-37` does.
4. `timeout 600 cargo test --no-fail-fast > /tmp/s4-battery.log 2>&1`, run once. Compare its reds to `.github/CI-KNOWN-RED.md` "dl8 cargo battery". A red that is not in that file is yours.
5. `ps -eo pid,pcpu,rss,args | grep -E '[d]l8 '` before and after.
6. The slices done, and the slices missing.

## Ownership

Owned:

- `src/_6_eval/_7_sqlite_eval.rs` and `src/_6_eval/_8_functions.rs` (new)
- `src/_6_eval/mod.rs`
- `src/_6_eval/_5_evaluate.rs`, the switch only
- `src/_5_reify/_7_sqlite.rs`, extraction only, with its emit output unchanged
- `src/_9_runtime/_1_sqlite.rs`, a register hook only
- `src/_9_runtime/_4_store_schema.rs` (new) and `src/_9_runtime/mod.rs`
- `src/bin/dl8.rs`, the `EmitTarget::Store` arm only
- `std/store.dl7` (new; this one file inside `std/`)
- `oracle/store/` (new)
- `tests/_27_eval_sqlite.rs` and `tests/_28_store_schema.rs` (new)
- `tests/_26_bounded_loops.rs`: its `UNBUDGETED` list may only shrink
- `Cargo.toml` deps block
- `docs/failure-modes.md`

Forbidden: every other path, including:

- `src/_0_read/`, `src/_2_lower/`, `src/_3_check/`
- `src/_1_macrotime/` (step 5) and `src/_4_comptime/` (step 6)
- `prelude/`, `macrotime/`, every other file in `std/`
- `oracle/eval`, `oracle/macrotime`, `oracle/compile`
- `book/`, `AGENTS.md`, `CLAUDE.md`, `sqlite_ivm/`

A needed sqlite_ivm change is a stop-and-report that carries the exact SQL
sqlite_ivm rejected. Never run `cargo fmt` on the crate. Run
`rustfmt --edition 2021 <owned file>` only. `git diff --stat origin/main...HEAD`
must list only owned files (and `Cargo.lock`).

## Laws, inline

- Every loop and every recursion is bounded. A `loop {}`, `while`, or
  recursive fn in `src/**` carries an explicit budget (`for _ in 0..LIMIT`, a
  depth counter, a row-set repeat check) and stops with a named diagnostic
  when the budget is hit. The bound is a constant with a comment saying what
  it protects. `tests/_26_bounded_loops.rs` enforces this.
- No join code in Rust. A join, a filter over rows, or a fixpoint is SQL. Rust
  moves rows in and out, and implements the `dl_*` scalar functions only.
- N+1 never. Collect the set, then run one multi-row `INSERT` per chunk.
  Every statement runs inside `sql()`.
- Read `.claude/skills/sql-relational-design/SKILL.md` and
  `.claude/skills/sqlite-costs/SKILL.md` before any DDL. Use surrogate INTEGER
  keys. Natural TEXT keys live once, in a dictionary table.
- No `eprintln!` in `src/**`. Use `tracing` only.
- Comments state only what the code cannot show. No dates, arc references, or
  change-log narrative.
- Use descriptive names. Interfaces carry the `I` prefix. Say products and
  rows, never "rel" in prose.
- No em dashes. Banned words in prose and identifiers: provenance, substrate,
  load-bearing, regime, "ground truth" (say oracle), refusal.
- An unbuilt construct is `not_built_yet`, with the throw site cited. It is
  never described as a language limit.
- No subagents. No background processes left running. Nothing may seize the
  machine.

## Report

```bash
boop beep --no-wait --as feat-eval-sqlite-step4 plan-eval-on-sqlite-ivm "step 4: PR #<n>, slices <done>/<4>, sqlite oracles <pass>/<total>, rust oracles <pass>/<total>, battery <pass>/<total>, sql spans <count> on 0_transitive, red: <list or none>, missing: <list>"
```
