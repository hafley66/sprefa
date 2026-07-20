# V6 storage crate — backend-neutral storage API, repo/rev everywhere

## Context

The schema currently lives in code, in four places, with no migration
framework:

- `_meta` system tables: `src/engine/meta.rs:207-412`; "migrations" are ad-hoc
  `ensure_column` probes and drop-rebuild blocks (`meta.rs:416-549`).
- `rel_*` tables generated from `RelDecl`s: `src/engine/declare.rs:298-356`.
- Family shadow schemas: `src/storage/call.rs:8-95`,
  `src/engine/cold_stage.rs:42-51`.

Verified against a live root db (505 tables): **only 86 tables (17%) carry
`repo` or `rev`; 418 lack both.** Hot rels (`rel_call_edge`, `rel_df_edge`,
`rel_df_node`) smuggle repo identity inside qualified sym strings instead of
columns. This in a tool whose entire purpose is cross-repo networks.

The V5 seam (`src/db.rs` ~30 verbs, `src/storage.rs` trait with one impl,
`.conn()` down to 12 uses) centralized *execution* but not *query text*: SQL
strings are built at call sites across ~40 files, and the codebase is bound
to SQLite idioms all the way down (WAL pragmas, `sqlite_master`, WITHOUT
ROWID classifiers). The seam's own track record (700 raw sites → 0 via
`.dl/no-new-rusqlite.dl`) proves rails work here — but the seam was drawn at
statement-execution altitude, and the API still *smells like SQLite*.

Owner ruling (2026-07-19): the backend is moot. The API must let us make and
query the way we want, against any database. This plan makes the backend
**configuration, not identity**.

## Decisions

**Backend is configuration, not identity.** `Store`'s public API speaks
rels, rows, keys, plans, `RepoRev` — no driver types, no SQLite-isms in any
signature. The backend is chosen at `open()`; the first backend is SQLite
(embedded, zero-ops). A second backend (Postgres for a network corpus,
DuckDB for analytics) is a new *internal* module — the trait gets extracted
then, per the standing ruling — but neutrality is enforced from day one by
the API rail below, not deferred.

**All SQL generation goes through sea-query.** Repository queries *and* the
`LoweredPlan → SQL` codegen are built as sea-query AST and rendered for the
configured dialect. sea-query is sea-orm's builder minus the ORM: typed
AST, SQLite/Postgres/MySQL renderers, no magic, no runtime. Rejected:

- **String SQL / `format!` splicing** — the V5 pain, ~40 files of it.
- **sea-orm / diesel** — ORM schema DSLs cannot express generated `rel_*`
  tables; magic and compile time for zero payoff.
- **sqlx `QueryBuilder`** — ties the builder to sqlx's async runtime; the
  builder must stay runtime-agnostic.
- **Exposing sea-query types in the public API** — the builder is internal;
  the API stays row-shaped.
- **Hand-writing our own SQL AST** — that is exactly the hand-rolling rule 1
  exists to prevent.

**Migrations: rusqlite_migration on the SQLite backend**, steps written in
sea-query DDL wherever expressible so they port to other dialects;
per-backend escape hatch stays inside the crate. `user_version` tracking,
round-trip tests. Rejected: more `ensure_column` (the pain being ended).

**The API is a concrete object, not a trait lattice.** "A proper DB API like
in Python" means a clean object with typed methods — not DI machinery. Per
the owner's standing ruling (`.dl/no-new-rusqlite.dl:5-8`): the seam becomes
a trait when a second backend arrives, and not one day sooner. Backend
neutrality is achieved by the *API shape* (no driver types leak), not by
premature abstraction.

**Two faces, one connection discipline.** `Store` is sync and lives on the
single writer thread — the engine talks to it directly, hot path stays fast
and dumb. `StoreHandle` is the cloneable async handle the server holds:
reads fan out to the reader pool, writes serialize through the writer
thread. No `spawn_blocking` per request.

**`RepoRev` on every table.** Every fact table carries `repo` and `rev`
columns; the API takes `&RepoRev` on every call so omitting it won't compile.
One documented exception class: content-addressed interning (`_strings`,
`_files`) is repo-agnostic *by design* and gets a written exemption in the
schema spec — everything else, no exemptions.

**Repository modules, not repository frameworks.** Per-domain modules
(`files`, `symbols`, `calls`, `types`, `df`, `meta`) own their queries
behind typed functions on `Store`. Engine code never sees a query builder,
let alone SQL.

**One corpus database.** All repos and revs as column partitions in one
database, replacing per-root `db.sqlite` files. Rejected: per-root files
with query-time federation — that keeps the 83% problem forever, and
cross-repo *is the product*.

**Interning becomes crate-internal.** `SymSink`/`_strings` move in; repo
qualification happens via columns at query time, not by baking it into sym
text.

## API sketch

```rust
pub struct RepoRev { pub repo: RepoId, pub rev: RevId }

/// Backend choice is config, made once at open. No driver or dialect type
/// appears anywhere below this line.
pub enum BackendConfig { Sqlite { path: PathBuf }, /* Postgres { dsn } … later */ }

/// Sync core — lives on the single writer thread. The engine uses this.
pub struct Store { /* one writer connection, backend-chosen */ }
impl Store {
    pub fn open(home: &Path, backend: BackendConfig) -> Result<Store>;
    pub fn migrate(&self) -> Result<SchemaVersion>;
    pub fn tx(&self) -> Result<Tx>;            // one writer at a time

    // reads — plain methods, snapshot-consistent, no driver types exposed
    pub fn rows(&self, rel: RelId, at: &RepoRev, key: &Key) -> Result<Rows>;
    pub fn one(&self, rel: RelId, at: &RepoRev, key: &Key) -> Result<Option<Row>>;
}

/// Async handle — what the server (and every transport handler) holds.
/// Reads fan out to the reader pool; writes serialize through the writer
/// thread. ~100 lines of channel plumbing, the one piece no library sells.
#[derive(Clone)]
pub struct StoreHandle { /* tokio mpsc to the store threads */ }
impl StoreHandle {
    pub async fn rows(&self, rel: RelId, at: &RepoRev, key: &Key) -> Result<Rows>;
    pub async fn write<F, T>(&self, f: F) -> Result<T>
    where F: FnOnce(&Store) -> Result<T> + Send + 'static, T: Send + 'static;
}

// Tx: insert / retract / upsert rows, exec_plan(&LoweredPlan), commit stamped
// with the tick's RepoRev. Internally everything renders through sea-query;
// outside, no SQL text, no driver types, no dialect names — the neutrality
// is greppable (rails below).
```

Honest boundary of the promise: the model is relational (rows in rels,
joins, plans executed as SQL). A pure-KV backend would need the store to
execute plans itself — out of scope until a real need arrives.

## Verification

- **Driver rail:** zero `rusqlite::`, `sqlx::`, `sea_query::` outside
  `sprefa-store`; zero `format!`-spliced SQL anywhere.
- **Neutrality rail:** `sprefa-store`'s public API (`pub fn`/`pub struct`
  signatures) names no driver or dialect type — greppable, CI-enforced.
- **Schema rail:** every table outside the documented interning class has
  `repo` + `rev` columns — a dl query over the store's own schema
  introspection, exit non-zero on violation. Coverage goes from 86/504 (17%)
  to 100% of non-exempt tables.
- **Migration tests:** empty → current; down → up round-trip; version
  monotonic; generated `rel_*` DDL participates in versioning.
- **Concurrency test:** concurrent `StoreHandle` reads during an active tick
  observe pre-tick snapshots and never block the writer.
- **Dialect smoke test:** the repository query suite renders and runs on the
  SQLite dialect; a Postgres-render pass (no server needed — render-only)
  proves the builder never leaked SQLite idioms.
- **Importer:** a one-off binary in the crate imports V5 per-root dbs into
  the corpus layout; run against a copy of the live 505-table db and diff
  row counts per rel.

## Staffing

One agent (opus-class), worktree under `.worktrees/`, base SHA `8d7b6092`
(branch `next`). Suite budget: migration round-trip tests + concurrency test
+ dialect smoke test + importer run + `scripts/verify.sh`. Lands after the
crate-map hollow types are reviewed.

<!-- todo(decision): enumerate the exact content-addressed interning tables exempt from the repo/rev rail (V5 candidates: _strings, _files) in the schema spec -->
<!-- todo(feature): V5→V6 importer — per-root db.sqlite files into corpus db -->
<!-- todo(perf): single-writer corpus tx throughput vs V5 per-root write parallelism — measure before deleting the per-root layout -->
<!-- todo(decision): sea-query coverage audit — can it express the fixpoint codegen's full SQL shape (CTEs, window fns, WITHOUT ROWID DDL)? Whatever it can't do is a written-excuse raw-fragment inside the crate. 2026-07-19 research says builders cover all of these except WITHOUT ROWID, which goes through .extra(); confirm against the real codegen -->
<!-- todo(decision): generated rel_* DDL vs rusqlite_migration's append-only model — hash the full generated DDL into one synthetic migration step, or use user_version as a schema-hash guard; decide in the store arc (impedance note from 2026-07-19 dep research, v6-deps skill) -->
