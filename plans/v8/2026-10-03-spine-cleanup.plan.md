# Spine cleanup: dl8, sqlite_ivm, ryi, scm++ (plan, 2026-10-03)

User: "come up with a plan to cleanup the common spine and interfacing and side effecting of the
3/4 crates its getting whiley". Inputs: read-only inventory (this session, 2026-10-03; numbers
below are from it), boundary memo `.claude/skills/2026-10-03-ryi-sprefa-boundary/SKILL.md`,
`plans/v8/2026-10-03-common-core.plan.md` (pick an interner, no bespoke one).

Ownership: sprefa side (dl8, sqlite_ivm) is executed here. ryi/scm++ items are filed as issues in
hafley-rs; sprefa never edits ryi.

## What the inventory found (top tangles)

| # | tangle | files | sprefa side | ryi side |
|---|---|---|---|---|
| 1 | global / thread-local state | ~45 | dl8 `_9b_ir_eval.rs:15-19` (`LAST_COUNTERS/ANSWERS/REQUESTS`), ivm-sqlite `2_terms.rs:10-11` atomics | scm ~35 sites (go.rs 8), ryi 5 files |
| 2 | interners | ~18 (15 interners) | dl8 6, ivm 5 | scm 5, ryi 2 |
| 3 | SQL text generators | ~17 | dl8 `_5_reify/_7_sqlite.rs`, `_9_runtime/_1_sqlite.rs`; ivm-sqlite 6 files; sqlite_ivm root `src/0c-0e_compile_*` (parses dl8's emitted SQL) | scm++ `_3_lower`, ryi 5 |
| 4 | env reads in library code | ~17 | `DL8_ENGINE` in eval (`_9b:369`), `DL8_READ_PATH`, `SQLITE_IVM_LIB`, `SPREFA_EXTRACT_BIN` | ryi 6, scm 7 (`HOME` read 6 times) |
| 5 | schema / DDL definitions | ~13 | ivm-ir `Relation`, dl8 `_2_declare`, runtime DDL | tsi registry, `schema.rs` |
| 6 | subprocesses inside library code | ~11 | dl8 `extract.rs:111` spawns ryi | scm spawns node/go/rustc/cargo/scip |
| 7 | error / diagnostic types | ~11 | dl8 `Diagnostic`, `Stop`; ivm-engine `EngineError` + a second set in ivm-sqlite `error.rs` | scm 4, ryi 1 |
| 8 | stratification / recursion | ~10 | dl8 `_6_eval/_2_stratify.rs:118` and `_3_check/_6_strata.rs:94` and demand strata | ryi `0_stratify`, reach view, scm `1_reach` |
| 9 | JSON formats for programs/terms | ~10 | dl8 6 files, ivm-ir serde | scm wire |
| 10 | two engine contracts in sqlite_ivm; two dependency checkouts | ~10 | ivm-sqlite legacy SQL-input API (`lib.rs:57-386`) beside `impl Engine`; dl8 links soopy/hafley-observe from the main hafley-rs checkout, ryi from the merge worktree | |

Also: 7 per-pass `cli()` functions inside dl8 library modules read files and `println!`
(`_0_read/_3_cli.rs:26` .. `_5_reify/mod.rs:57`); compile reads files from inside the driver
(`_8_driver/_0_read.rs:60`, `_4_project.rs:72,89`); `_6_eval/_8_functions.rs` is dead.

## Target shape

Pure core, effects at the edge. Three layers, one direction of arrows:

| layer | crates | may do |
|---|---|---|
| core (pure) | ivm-ir (plan IR + shared term/text layer), dl8 read/macrotime/lower/check/comptime/reify | compute; no env, no fs, no print, no threads, no statics |
| engines | ivm-engine (trait), ivm-dd, ivm-sqlite | own their storage and worker threads; no env |
| edge | dl8 bin, dl8 `_9_runtime` executors, ryi daemon | env, fs, network, subprocess, stdout, the one config struct |

## Type signatures, then bodies (pseudo-code)

```rust
// edge: read once in the bin, passed down by value. Replaces every std::env::var in library code.
pub struct Config {
    pub engine: Backend,             // was DL8_ENGINE read in _9b_ir_eval.rs:369
    pub sqlite_ivm_lib: Option<PathBuf>,   // was SQLITE_IVM_LIB, _1_sqlite.rs:73
    pub ryi: RyiEndpoint,            // daemon socket (boundary memo), was SPREFA_EXTRACT_BIN subprocess
    pub observe: ObserveConfig,
}
impl Config { pub fn from_env() -> Result<Config, ConfigError> /* bin only */ }

// edge: compile input. The driver stops reading files itself.
pub trait Sources {
    fn read(&mut self, path: &Path) -> Result<Bytes, Stop>;     // fs, or an in-memory map in tests
    fn tsi_stream(&mut self, path: &Path) -> Result<Bytes, Stop>;
}
pub fn compile(u: &mut Universe, sources: &mut dyn Sources, cfg: &Config, fx: &mut dyn FnMut(Event))
    -> Result<Compile, Stop>;
// body: same chain as lib.rs:137, every read goes through `sources`.

// core: results carry what the thread-locals carried (LAST_COUNTERS / ANSWERS / REQUESTS).
pub struct Evaluated { pub closure: Closure, pub counters: Counters, pub answers: Vec<Answer>, pub requests: Vec<Request> }
pub fn evaluate(session: &mut dyn Session, program: &Program) -> Evaluated;

// core: one relation schema, from the .tsp (boundary memo C2). ivm-ir Relation, dl8 declarations of
// external tables and every DDL emitter read it.
pub struct RelSchema { pub name: Id, pub columns: Vec<(Id, Ty)>, pub key: Vec<usize> }
pub fn schemas_from_tsp(text: &str) -> Result<Vec<RelSchema>, Diagnostic>;

// core: one term/text layer shared by dl8 and the engines (library picked by the common-core spike:
// lasso or ra_ap_intern). Engines receive ids, never re-intern.
pub trait Terms {
    fn text(&mut self, s: &str) -> Id;
    fn term(&mut self, functor: Id, args: &[Id]) -> Id;
    fn get(&self, id: Id) -> TermRef<'_>;
}
pub trait Engine {                       // ivm-engine, intern_text/intern_terms/text/any_value removed
    fn install(program: &Program, terms: SharedTerms) -> Result<Self, EngineError>;
    fn settle(&mut self, frontier: Frontier) -> Result<Delta, EngineError>;
    fn snapshot(&self, rel: RelId) -> Result<Vec<(Row, W)>, EngineError>;
}

// core: one plan IR, renderers beside it (boundary memo D3).
pub trait Render { type Out; fn render(p: &Program, schemas: &[RelSchema]) -> Result<Self::Out, Unsupported>; }
pub struct BatchSql { pub dialect: Dialect }   // DuckDB | SQLite; one-shot, no IVM tables
// incremental rendering stays inside ivm-sqlite (its 3_nodes.rs)
```

## Instance timelines

- `Config`: built once in `main`, lives for the process, passed by `&`.
- `SharedTerms`: one per process (or per watch session), created before reading; dl8 lowering,
  comptime, IR lowering and every engine hold a handle; append-only; dropped at exit. Kills
  `install_terms` (`_9b:467`) and `decode_terms` (`_9b:405`).
- `Session`/engine: one per install as today; its counters/answers return in `Evaluated` instead of
  thread-locals.
- `RelSchema`s: loaded once from `.tsp` at compile start; immutable.

## Storage: reads, writes, uniqueness

- Terms/text: unique by content in `SharedTerms`; persisted ids are the xxh3-64 content ids
  (boundary memo); stores write `dict(id, text)` / `term(id, functor, args)` with id PK, `INSERT OR
  IGNORE`.
- Runtime store (`_9_runtime`): tables from `RelSchema`, not hand-written DDL; one connection owner
  (`SqliteRowStore`).
- Engine storage: owned inside each engine (done: containment lane); no engine reads env or a
  shared connection.

## Lanes (fresh agent each, sequential, sprefa side)

| # | lane | removes | done when |
|---|---|---|---|
| 1 | effects to the edge in dl8 | env reads in `_9b`, `_0_read/_3_cli.rs`; 7 library `cli()`s (move to `bin/dl8.rs`); thread-locals `_9b:15-19`; file reads in `_8_driver` (`Sources`); dead `_8_functions.rs`, `read_source_fact_files`, `read_dir_tree` | `git grep -n 'std::env::var\|println!\|thread_local!' -- src ':!src/bin'` empty except listed edge files; a contract test enforces it (like `_8_contracts`) |
| 2 | one engine contract in sqlite_ivm | ivm-sqlite legacy SQL-input API (`lib.rs:57-386`, `error.rs`, `plan.rs`, `catalog.rs`, `composition.rs` where only the legacy path uses them), root `src/0c-0e_compile_*`; dl8 `_5_reify/_7_sqlite.rs` + runtime vtab path move to `ivm_ir` engines | dl8 runtime runs on `Engine`; no SQL text crosses dl8 -> sqlite_ivm; second `EngineError` gone |
| 3 | interner spike, then shared terms | per common-core plan: lasso vs ra_ap_intern; then `SharedTerms` in ivm-ir; Engine intern_* methods removed; dl8 `_9b` install/decode removed | one term layer; registry build time not worse |
| 4 | one schema | `RelSchema` from `.tsp` (TypeSpec importer in dl8); runtime DDL, ivm-ir `Relation`, external-table import (recursion lab arm S) use it | DDL generated, not hand-written, in dl8 runtime |
| 5 | one stratifier, one order/agg enum | `_3_check/_6_strata.rs` uses `_6_eval/_2_stratify.rs`; dl8 `Order`/`AggregateKind` replaced by ivm-ir's | one SCC implementation in dl8 |
| 6 | dependencies through kellnr | dl8's soopy / hafley-observe path deps into the main hafley-rs checkout | pinned registry versions; no cross-repo path deps (boundary memo K2) |
| 7 | batch renderer | `BatchSql` beside ivm-ir (DuckDB, SQLite dialects) | DuckDB bench part B runs through it |

ryi / scm++ side (filed as one hafley-rs issue, their lanes): ~40 statics in scm, env reads (`HOME`
x6, `DL_TRAIL`/`DL_TRACE_SUMMARY` read in two crates), subprocesses inside `read/lang` resolvers,
library-level stdout in `edit/`, scm++ lowering to ivm-ir instead of SQL text, ids as content hash.

## Order and why

1 first: pure-core rules make every later lane testable without fs/env. 2 before 3: one engine
contract before changing what crosses it. 3 before 4: schemas name columns by `Id`. 6 any time
(small). 7 last (needs 3-4).
