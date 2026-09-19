# eval on sqlite_ivm: the relational core moves into SQLite

Driver lane: opus. Implementation lanes: `glm53f-omp`, one at a time, spawned by
the driver through `boop beep lane create`. Coordinator: `sprefa-coordinator`
(Fable) grades every PR. User word 2026-09-18, verbatim intent: "no more
unbounded non relationally stratified things", "no more relational and dbsp
shit in rust, it's in the sqlite now", "IVM is in its name", "have a tracer at
every sql boundary", "mmap wal whatever and large paging, i don't want to fuck
with sqlite tuning again", "do not allow anything to contend my ram and cpu".

## Machine laws (blocking defects, every lane, every command)

- One lane alive at a time. The driver never spawns a second implementation
  lane before the first is retired and graded. Never `Agent` subagents.
- Every `dl8` invocation: `timeout 10 ...`. A timeout IS the result; write it
  down, never rerun longer. No bisect loops, no `for i in 1 2 3` timing loops
  over a debug binary, no background processes left running.
- One cargo at a time. `~/.cargo/config.toml` caps jobs at 4; never override.
  Build once per step; never `touch src/lib.rs && cargo build`.
- Battery: `cargo test --no-fail-fast` once per step, in the foreground, with
  `timeout 600`; `_22_book::chapter_executors` measured alone, three times.
- Check `ps -eo pid,pcpu,rss,args | grep dl8` before and after every step. A
  leftover process is killed and ledgered in `docs/failure-modes.md`.

## Why (measured 2026-09-18, release build, `fixtures/openapi/todo.dl7`)

| tree | wall | where |
|---|---|---|
| main `b7dd3f140` | 0.13 s | five phases, 63 ms in spans |
| `feat/prelude-macrotime` step 3 (prelude through macrotime) | 0.24 s | P1 123 ms |
| step 5 (prelude uses `[..]`) | never returns | P1 round 1 |

Cause, `src/_6_eval/_5_evaluate.rs`: `positive_solutions` (`:250-256`) calls
top-down `demand` (`:283-322`) for every bound goal on a derived relation;
the memo is fresh per rule firing (`:379`), `demand` plans are `Range::All`
(`:308`) so semi-naive deltas are defeated, and strata are ignored. One firing
of `ambiguous_return` (`macrotime/2_caret.dl7`, the 7-goal rule with
`int.lt`) over the prelude's 16151 syntax rows = 48353 demand calls, 246 ms;
in round 1 a 12-goal rule fires once per delta position, each firing redoing
its whole demand tree. Debug opt-level 0 multiplied this ~100x (59 s, then
hours). Probe worktree with per-rule timing: `.boop-worktrees/probe/p1-hot`
(`src/_6_eval/_5_evaluate.rs` `dl8::fire` target); delete it when step 1 lands.

## Target shape

| today | after |
|---|---|
| `_6_eval/_5_evaluate.rs` bottom-up + top-down `demand`, hand indexes `_3_table.rs` | rules lowered to `sqlite_ivm` virtual tables (existing emitter `src/_5_reify/_7_sqlite.rs`, 1503 lines, one vtab per derived relation, upstream relations as CTEs, positive `WITH RECURSIVE` per stratum) |
| seeds as `Vec<Row>` in memory | seeds `INSERT`ed into source tables (`src/_9_runtime/_1_sqlite.rs` `SqliteRowStore`, 647 lines) inside one transaction; sqlite_ivm maintains results in the same transaction |
| kernel goals in Rust (`_4_kernel.rs`: `Nil Cons StrNil StrCons EdgeRef Intern Int(IntCmp) IntAdd TermLt CountStep MinStep MaxStep`) | deterministic scalar functions registered on every connection with `rusqlite::Connection::create_scalar_function` (sqlite_ivm README "Load deterministic registered functions on every connection"); `Intern` = a source table with `UNIQUE` on the natural key, the `intern`/`intern_snapshot` requests read back as rows |
| macrotime waves: `evaluate` from scratch per wave (`_1_macrotime/_4_expand.rs:122`) | wave = insert the new syntax rows, read the claim view; sqlite_ivm does the delta |
| comptime rounds re-evaluate (`_4_comptime/_2_rounds.rs`) | round = insert answers, read pending effects |
| `Range::All / Old / Delta` plans, `demand`, `memo` | deleted |
| in-memory store per compile | one SQLite db per compile: `:memory:` for oracles and tests, a file under `--db` |

The evaluator is the ONE bespoke layer the repo allows; it becomes lowering
plus a thin `IEvaluate` over rusqlite. No new join code in Rust, ever again.
`sql-relational-design` and `sqlite-costs` skills are mandatory reads before
any DDL (surrogate INTEGER keys, natural keys once in a dictionary table,
no composite TEXT PRIMARY KEY).

## Connection contract (one function, every connection, no per-site tuning)

`src/_9_runtime/_1_sqlite.rs` grows `fn open(path) -> Connection` and every
connection in the crate goes through it:

```sql
PRAGMA page_size = 65536;        -- before the first table, on a new file
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA mmap_size = 1073741824;
PRAGMA cache_size = -262144;     -- 256 MiB, negative = KiB
PRAGMA temp_store = MEMORY;
PRAGMA recursive_triggers = ON;  -- sqlite_ivm requires
PRAGMA trusted_schema = ON;      -- sqlite_ivm requires
```

then `load_extension(libsqlite_ivm)` (`tests/_18_sqlite_emit.rs:29-49` already
locates it; lift that into the runtime), then the scalar functions. A COUNT
test asserts `PRAGMA journal_mode` returns `wal` and `mmap_size` the value.

## Tracer at every SQL boundary

`hafley-observe` 0.1.1 is linked (`Cargo.toml:13`) and flushes on exit
(`src/bin/dl8.rs:137`). Every statement the crate runs goes through one
`fn sql<T>(conn, name, sql, f) -> T` wrapper in `_9_runtime/_1_sqlite.rs`:
`tracing::info_span!("sql", name, rows, ms)` around the call, `rows` recorded
after. Batches (`INSERT` of a seed set) are one span with the row count.
`rusqlite::Connection::trace_v2` is NOT used (fires per step, floods). Receipt:
`just trace-up && just trace compile fixtures/openapi/todo.dl7 && just
trace-down && just trace-query` lists every `sql` span with its parent phase.

## Steps

| # | step | lane | receipt |
|---|---|---|---|
| 1 | stop the bleed on `_6_eval`: `demand` only for relations whose rules need bound head args (kernel constructor or negative goal over head vars, computed once in `evaluate_into`); memo moves to `Context`, one per round. Keep semantics: every oracle byte-identical | glm | `cargo test --test _0_eval_oracle --test _2_macrotime_oracle --test _8_compile_oracle` PASS; COUNT test: demand calls for `ambiguous_return` over the prelude under 500; P1 on `feat/prelude-macrotime` tree under 100 ms release. PR |
| 2 | `open()` contract + `sql` span wrapper; every existing connection site rerouted; pragma COUNT test; `just trace-query` receipt pasted | glm | PR |
| 3 | design page `plans/v8/2026-09-18-eval-on-sqlite-ivm.design.md` + `.visual.human.unga.md`: schema for syntax rows, `:` edges, intern dictionary, effect pending, snapshot; every kernel goal's SQL spelling; stratified negation as separate vtabs; what sqlite_ivm rejects (README "accepted grammar") and the rewrite for each; the `IEvaluate` signature; storage layout, then sequence of reads and writes, then uniqueness conditions | opus (driver writes it) | coordinator + user read before step 4 |
| 4 | `IEvaluate` over rusqlite: seeds in, results out, oracles byte-identical, `_6_eval/_5_evaluate.rs` behind a flag for one PR | glm | oracle PASS both paths; `sql` spans in trace |
| 5 | macrotime waves on the store | glm | `_2_macrotime_oracle` PASS; wave count and rows per wave in spans |
| 6 | comptime rounds on the store | glm | `_6_comptime_oracle`, `_23_comptime_effect` PASS |
| 7 | delete the Rust evaluator, `demand`, `memo`, `Range`, `_3_table.rs`; `_6_eval` keeps terms, program, stratify, json | glm | `wc -l src/_6_eval/*.rs` before/after; battery |
| 8 | `feat/prelude-macrotime` steps 3-7 replayed on the new core; P1 under 100 ms | glm | numbers, PR |

Every step: its own branch from `origin/main`, its own PR, driver grades the
tree (`git log`, `git status`, forbidden-path diff, battery) before hailing the
coordinator. `rc=0` means nothing.

## Ownership

Owned: `src/_6_eval/`, `src/_9_runtime/_1_sqlite.rs`, `src/_5_reify/_7_sqlite.rs`,
`src/_1_macrotime/_4_expand.rs`, `src/_4_comptime/_2_rounds.rs`, `tests/`,
`oracle/` (refreeze only with a pasted delta), `plans/v8/2026-09-18-eval-on-sqlite-ivm.*`,
`docs/failure-modes.md`, `Cargo.toml` deps block only.
Forbidden: `src/_0_read/`, `src/_2_lower/`, `src/_3_check/`, `prelude/`,
`macrotime/`, `std/`, `AGENTS.md`, `CLAUDE.md`, `sqlite_ivm/` (a needed change
there is a stop-and-report with the exact SQL sqlite_ivm rejected).

## Laws inline

No `eprintln!`. Comments state only what the code cannot show. Descriptive
variable names. No em dashes. Banned words: provenance, substrate,
load-bearing, regime, "ground truth". Products and rows, never "rel". Interfaces
carry the `I` prefix. N+1 never: collect the set, one multi-row `INSERT`.
Formerly-quadratic paths get COUNT tests. Async stays out; rusqlite is sync.

## Report

```bash
boop beep --no-wait --as <lane> sprefa-coordinator "eval-on-sqlite-ivm step <n>: PR #<n>, oracles <pass>/<total>, battery <pass>/<total>, todo.dl7 compile <ms> release, sql spans <count>, red: <list or none>"
```

## Addendum 1 (user 2026-09-18 evening): step 2 stress-tests its own helpers

`tests/_25_sqlite_contract.rs`, integration through `open()` and `sql()`, a
file db under the test's temp dir, every case one function, every number a
COUNT or a ratio against a formula, never a wall-clock threshold. Reference
constants: `.claude/skills/sqlite-costs` (measured on this machine).

| case | shape | expected, as a formula | why it exists |
|---|---|---|---|
| N+1 insert vs one multi-row INSERT in one transaction | 10k rows | per-statement path >= 20x the batched path in `sql` span ms; batched under 50 ms | the beginner scaling defect, the N+1 law |
| autocommit vs explicit transaction | 10k single-row inserts | autocommit >= 50x; the span count equals the statement count both ways | fsync per statement |
| lookup on indexed vs unindexed column | 100k rows, 1k probes | unindexed ms / indexed ms grows with n (measure at 10k and 100k, ratio at 100k >= 5x the ratio at 10k) | SEARCH vs SCAN, `EXPLAIN QUERY PLAN` asserted, not timed |
| join with and without the index on the join key | 10k x 10k | `EXPLAIN QUERY PLAN` has no `SCAN` on the inner side when indexed | the evaluator's own joins |
| `PRAGMA journal_mode`, `page_size`, `mmap_size`, `cache_size` | read back after `open()` | exact values from the contract table | connected correctly |
| cache pressure | insert 64 MiB of rows with `cache_size` at 8 MiB then at 256 MiB | RSS delta (getrusage `ru_maxrss` before and after) under `cache_size` + 32 MiB both times; second run's span ms <= first | memory ceiling is the pragma, not the data |
| WAL checkpoint | 50k inserts, then `PRAGMA wal_checkpoint(TRUNCATE)` | wal file size 0 after; span present | WAL growth is bounded by us |
| Rust cost vs SQLite cost at time T | one `phase` span holding three `sql` spans | phase ms - sum(sql ms) = Rust ms, asserted >= 0 and reported in the trace query as its own column | the differentiation you asked for |

Receipt for the step: the eight rows pasted with their numbers from one run,
plus `just trace-query` output showing `phase`, `sql`, `rust_ms` columns.

## Addendum 2 (user 2026-09-18 evening): the store schema is declared once, in dl7

The evaluator's own tables (syntax rows, `:` edges, intern dictionary, effect
pending, snapshot, per-relation products) are declared as dl7 products in
`std/store.dl7`. dl8 lowers them, as it lowers any product today
(`src/_5_reify/_7_sqlite.rs` for DDL), to two emitted files: the SQLite DDL
and the Rust row structs plus `IRowStore` bindings. TypeSpec shape: one
declaration, two emitters, no hand-written duplicate of a column name or a
type anywhere in `src/_9_runtime/`.

Bootstrap: the emitter runs at build time (`build.rs` invokes the lowering on
`std/store.dl7` into `OUT_DIR`) and the two outputs are ALSO frozen under
`oracle/store/` as goldens, diffed by a test; a change to `std/store.dl7`
refreezes in its own commit with the delta pasted. The compiler never needs a
running store to emit the store.

Step 3's design page carries this as its first section: the `std/store.dl7`
text, the emitted DDL, the emitted Rust, side by side. It is a design fork
for Chris (Rust emitter surface: structs only, or structs + insert/select
functions), decided before step 4.
