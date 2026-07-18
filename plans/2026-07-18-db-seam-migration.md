# Db seam migration: every SQL statement behind named methods on Db

## Context

Two 2026-07-18 facts drive this arc:

- The ColdExtract poison-job error `Invalid function parameter type Null at
  index 0` arrived with no statement text and no table name; diagnosis cost
  hours. Every statement failure must now carry the statement head and the
  table it ran against.
- The containment rail `.dl/no-new-rusqlite.dl` (same day) grandfathered the
  existing spread instead of removing it. This arc burns the grandfathering
  to zero. Definition of done: every baseline row in all three ratchets
  ratchets to 0 and is deleted; `Db::conn()` is deleted.

Ground truth (sums of the rail's own baseline rows, measured 2026-07-18):

| ratchet | files | sites | worst files |
|---|---|---|---|
| `rusqlite_baseline` | 30 | 177 | rpc.rs 23, daemon_read.rs 21 |
| `conn_baseline` | 39 | 332 | engine/meta.rs 60, engine/derive.rs 25 |
| `sqlite_swallow_baseline` | 8 | 22 | engine/meta.rs 7 |
| (untracked by rail) `tests/it/*.rs` | 9 | ~33 | rail scans `src/**` only |

The current seam (`src/db.rs`, 1047 lines) owns: `insert_rows`, `reload_rel`,
`exec`, `execute_batch`, `prepare`, `query_row`, `rel_stats`, `flush_syms`,
`flush_pending_syms`, `tick_begin`/`tick_end`, `take_write_ledger`/
`clear_write_ledger`, `is_autocommit`, and `conn()` — the escape hatch this
arc deletes. `open_read_only` hands a raw `rusqlite::Connection` to
`daemon_read.rs`. `src/storage.rs` holds a pre-existing `Storage` trait used
by `engine/extract/call.rs`'s `CallStore` (see Decisions).

Call-site shapes were sampled from the six heaviest files
(`engine/meta.rs`, `effect.rs`, `daemon_read.rs`, `engine/derive.rs`,
`engine/declare.rs`, `engine/rpc.rs`) plus `engine/pipeline/{apply,full_sources,
source_stage}.rs` on 2026-07-18; the API below is clustered from those shapes.

User rulings that bind the design:

- The seam stays a STRUCT welded to SQLite. No trait until a second backend
  exists.
- Databases are ephemeral: migrations optional, wipe+rebuild acceptable.
- N+1 law: seam methods are plural/batch-shaped — collect rows, one
  statement; never per-row in a loop.
- Banned identifier words (no new method/type/field/param may contain them):
  `provenance`, `substrate`, `load-bearing`, `regime`.

## Decisions

- **New API lands on `Db`'s inherent impl in `src/db.rs`.** Rejected: a new
  `DbOps` trait (user ruling); extending the existing `Storage` trait
  (ruling; `Storage` stays exactly as-is for `CallStore` — collapsing it is
  out of scope, see todo).
- **`SqlVal`: a seam-owned parameter/row value type.** Call sites never name
  a rusqlite type again, so the ratchet can reach zero. Rejected:
  `impl rusqlite::Params` method params (keeps `rusqlite::` text at every
  call site); reusing `ast::Value` (no Real/Blob variant; an AST type should
  not grow storage concerns).
- **Every statement-issuing method takes `rel: &str` first** — the table the
  statement chiefly reads or writes. It feeds error context and the N+1
  counter key. Rejected: parsing the table out of the SQL (unreliable for
  joins and DDL scripts).
- **Error wrapping via `anyhow` context**: `sql failed on <table>: <head> —
  <cause>`. anyhow is the repo idiom; no new error enum. Rejected: a
  `SqlFailure` struct (more machinery, same output).
- **Row-mapping errors propagate.** Today's dominant read shape ends in
  `.filter_map(|x| x.ok())` — a silent row drop, the same failure class as
  the swallow incident. `query_rows` collects with `?`. Intended behavior
  change, called out in Layer 4.
- **Named transaction primitives (`begin_immediate`/`commit`/`rollback`) plus
  a closure `transact`.** Rejected: a `Transaction` guard type —
  `with_semantic_generation`'s closure needs `&mut Engine`, which a
  `&Db`-holding guard makes unborrowable. `unchecked_transaction` disappears
  from the codebase.
- **`ReadDb`: a read-only sibling struct in `src/db.rs` owns the daemon read
  path's connection.** `open_read_only` is deleted; `daemon_read.rs` stops
  naming rusqlite. Rejected: waiving daemon_read (largest read surface
  outside the counter).
- **No prepared-statement cache.** Prepare-per-call preserves today's
  behavior; a SQL-keyed cache has invalidation surface across schema-epoch
  wipes. Follow-up only if profiling shows a chunk-loop regression (todo).
- **`ensure_column` replaces every `ALTER TABLE ... ADD COLUMN` tolerance
  swallow** (`engine/meta.rs`): check `column_exists`, ALTER only when
  missing. The swallowed duplicate-column error was only masking the missing
  check.
- **Schema-migration blocks stay wipe+rebuild** (ephemeral-db ruling). This
  arc adds no data migration and removes none.
- **`rusqlite::Row` appears only inside closure bodies, type-inferred.**
  After a file's batch lands it contains no `use rusqlite`, no `rusqlite::`,
  and no `Connection`/`Statement`/`OptionalExtension`/`ToSql`/`params!`. The
  rail's regex is the floor; executors also grep for the bare type names.

## Layer 1 — seam API surface (type signatures)

Signatures clustered from the sampled call-site shapes. `Row` =
`rusqlite::Row`, imported in `db.rs` only; closure parameter types are
inferred at call sites, so `rusqlite` never appears outside the seam.

```rust
// ================= src/db.rs — seam-owned value + error support =================

/// One bound parameter or result cell, seam-owned. Call sites build these;
/// `rusqlite::types::Value` never leaves db.rs.
pub enum SqlVal { Null, Int(i64), Real(f64), Text(String), Blob(Vec<u8>) }
// From impls: &str, String, &String, i64, i32, u32, usize, f64, &[u8], Vec<u8>,
//   Option<T: Into<SqlVal>> (None -> Null).
impl SqlVal {
    pub fn from_json(v: &serde_json::Value) -> SqlVal;
    // The param mapping engine/rpc.rs::query_sql and daemon_read.rs::json_rows
    // duplicate today: String->Text, i64->Int, f64->Real, Null->Null,
    // other->Text(v.to_string()).

    pub fn to_json(&self) -> serde_json::Value;
    // Text->String, Int->Number, Real->Number, Null->Null, Blob->"<blob NB>"
    // (the rpc.rs::query_sql flavor).

    pub fn to_lossy_string(&self) -> String;
    // The engine/mod.rs::cell_as_string flavor: Null->"", Int/Real->to_string,
    // Text->clone, Blob->lossy utf8.
}

/// Statement head for logs and errors: whitespace-compacted, first 80 chars.
fn stmt_head(sql: &str) -> String;

/// THE error-context wrapper. Every statement failure leaves the seam as:
///   sql failed on <rel>: <stmt_head> — <rusqlite error>
/// The 2026-07-18 incident message becomes e.g.:
///   sql failed on _strings: INSERT INTO _strings ("id", "content") VALUES (?, ?) … — Invalid function parameter type Null at index 0
fn sql_err(rel: &str, sql: &str, e: rusqlite::Error) -> anyhow::Error;

/// "?, ?, …" n times — IN-list rendering for the *_in_chunks helpers.
pub fn holes(n: usize) -> String;

// ============================ src/db.rs — Db reads ============================

impl Db {
    pub fn query_one<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<T>;
    // prepare; bind params; query_row. Prepare/query errors -> sql_err(rel, sql).
    // read-closure errors -> context "row map on {rel}: {head}". Exactly one row
    // required: QueryReturnedNoRows propagates, context-wrapped like the rest.

    pub fn query_opt<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                        read: impl FnOnce(&Row<'_>) -> Result<T>) -> Result<Option<T>>;
    // Same; zero-or-one row. Absorbs every `use rusqlite::OptionalExtension;
    // … .optional()` site (declare.rs::refresh_every and friends).

    pub fn query_rows<T>(&self, rel: &str, sql: &str, params: &[SqlVal],
                         read: impl FnMut(&Row<'_>) -> Result<T>) -> Result<Vec<T>>;
    // prepare; query; map each row; collect with `?` — NEVER filter_map(ok).
    // The dominant shape: prepare + query_map + collect (rpc.rs, derive.rs,
    // effect.rs, meta.rs).

    pub fn for_each_row(&self, rel: &str, sql: &str, params: &[SqlVal],
                        f: impl FnMut(&Row<'_>) -> Result<()>) -> Result<()>;
    // Streaming fold for scans that must not materialize a Vec (derive.rs's
    // graph-load loops). Prefer query_rows when a Vec is built anyway.

    pub fn query_values(&self, rel: &str, sql: &str, params: &[SqlVal])
        -> Result<Vec<Vec<SqlVal>>>;
    // Rows as seam values — the rpc.rs::query_sql / daemon_read.rs::json_rows /
    // string_rows shape. Callers map each cell with SqlVal::to_json or
    // to_lossy_string. Kills three drift-prone copies of the same conversion.

    pub fn query_in_chunks<T>(&self, rel: &str, sql: impl Fn(usize) -> String,
                              head: &[SqlVal], keys: &[SqlVal],
                              read: impl FnMut(&Row<'_>) -> Result<T>) -> Result<Vec<T>>;
    // SELECT … WHERE k IN (<n holes>), chunked so head.len() + n <= PARAM_BUDGET.
    // sql(n) renders the statement for n keys (use holes(n)). One logical op:
    // bump once per call, not per chunk. Absorbs derive.rs's 30k-chunk
    // `_strings` render reads.

    pub fn digest_rows(&self, rel: &str, sql: &str, params: &[SqlVal]) -> Result<[u8; 32]>;
    // Moves engine/meta.rs::digest_of_query into the seam, byte-identical:
    // per row, blake3 over tagged cells ("i"+le / "r"+le / "t"+bytes /
    // "b"+bytes / "n", 0x00 separator), then XOR-fold the row hash into the
    // accumulator. rel_content_digest and carry_differs both call this.

// ============================ src/db.rs — Db writes ===========================
// All plural by construction; a loop of singleton calls trips the N+1 scream.

    pub fn exec(&self, rel: &str, sql: &str) -> Result<usize>;
    // Today's parameterless exec + rel param + sql_err wrap. Signature change;
    // commit 0 fixes the existing `db.exec(...)` call sites.

    pub fn exec_params(&self, rel: &str, sql: &str, params: &[SqlVal]) -> Result<usize>;
    // One prepared execute with bound params; returns rows-affected (the
    // changes()==1 exactly-once claim in effect.rs reads this).

    pub fn exec_in_chunks(&self, rel: &str, sql: impl Fn(usize) -> String,
                          head: &[SqlVal], keys: &[SqlVal]) -> Result<usize>;
    // UPDATE/DELETE … WHERE k IN (<holes>), chunked like query_in_chunks; sums
    // rows-affected; one logical bump. Absorbs park_orphan_effects,
    // requeue_orphaned_effects, gc_done_effects, the drain mark-done loop,
    // delete_rel_digests — every `Vec<&dyn ToSql>` + manual-placeholder site.

    pub fn upsert_rows(&self, table: &str, cols: &[&str], key_cols: &[&str],
                       update_cols: &[&str], rows: &[Vec<SqlVal>]) -> Result<usize>;
    // Chunked multi-row INSERT INTO table (cols) VALUES … ON CONFLICT(key_cols)
    // DO UPDATE SET update_cols = excluded.* (empty update_cols -> DO NOTHING).
    // empty rows -> Ok(0). Absorbs save_rel_digests and the _carry_meta upserts.

    pub fn execute_batch(&self, rel: &str, sql: &str) -> Result<()>;
    // Today's wrapper + rel param + sql_err. DDL scripts only; rel = the first
    // object the script creates or drops. NOT for tx control (use the named
    // primitives below).

    // insert_rows / reload_rel / flush_syms / flush_pending_syms:
    // signatures unchanged; bodies gain sql_err wrapping (rel = table).
    // retract_rows lives in the `impl Storage for Db` block in src/storage.rs —
    // same treatment there (seam file, signatures unchanged, sql_err wrapping).

// ====================== src/db.rs — introspection / pragmas ===================

    pub fn column_exists(&self, table: &str, column: &str) -> Result<bool>;
    // SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2 via query_opt.

    pub fn ensure_column(&self, table: &str, column: &str, alter_ddl: &str) -> Result<bool>;
    // column_exists -> Ok(false); else exec(table, alter_ddl) -> Ok(true).
    // Replaces every `let _ = execute("ALTER TABLE … ADD COLUMN …")` in meta.rs.

    pub fn secondary_indexes(&self, table: &str) -> Result<Vec<(String, String)>>;
    // (name, sql) FROM sqlite_master WHERE tbl_name=?1 AND type='index'
    //   AND sql IS NOT NULL ORDER BY name.
    // Dedups reload_rel's inline copy and derive.rs step 7's copy.

    pub fn schema_objects(&self, name_like: &[&str]) -> Result<Vec<(String, String)>>;
    // (name, type) FROM sqlite_master WHERE type IN ('table','view') AND name
    // LIKE any(patterns). ensure_meta's epoch-wipe scan.

    pub fn pragma_i64(&self, name: &str) -> Result<i64>;
    // PRAGMA <name> as one i64 (user_version, temp_store). PRAGMAs can't bind
    // params; `name` is a compile-time literal at every call site.

// ======================= src/db.rs — transaction control ======================
// Ownership rule: who begins, closes; a helper never closes an outer tx.

    pub fn begin(&self) -> Result<()>;            // execute_batch("_tx", "BEGIN")
    pub fn begin_immediate(&self) -> Result<()>;  // execute_batch("_tx", "BEGIN IMMEDIATE")
    pub fn commit(&self) -> Result<()>;
    pub fn rollback(&self) -> Result<()>;

    pub fn transact<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T>;
    // !is_autocommit -> work() (caller owns the outer tx). Else: begin_immediate;
    // commit on Ok; rollback on Err; on panic, rollback then resume_unwind — the
    // apply.rs control flow, seam-owned. Engine's with_semantic_generation keeps
    // its own copy ONLY because its closure needs &mut Engine; its SQL becomes
    // the named primitives above. Non-Engine owners (jobq, daemon, rels) call
    // transact.

    pub fn is_autocommit(&self) -> bool;  // unchanged, no SQL
}

// ====================== src/db.rs — the daemon read path ======================

/// Read-only connection owner for daemon_read. Counted into the process-wide
/// profile hook; not part of per-tick N+1 counting (a read RPC has no tick).
pub struct ReadDb { conn: Connection }
impl ReadDb {
    pub fn open(path: &str) -> Result<ReadDb>;
    // Was db::open_read_only: READ_ONLY|NO_MUTEX flags, 1s busy_timeout, the
    // same scalar-fn registry (read-only sprf_sym_intern: id of text, no
    // intern queue).

    pub fn query_one<T>(...) -> Result<T>;      // same shapes as Db's reads,
    pub fn query_opt<T>(...) -> Result<Option<T>>; // minus the rel-keyed bump
    pub fn query_rows<T>(...) -> Result<Vec<T>>;
    pub fn query_values(...) -> Result<Vec<Vec<SqlVal>>>;
    // No write methods. daemon_read's json_rows = query_values + to_json;
    // string_rows = query_values + to_lossy_string.
}
```

Final burn-down deletes from `Db`: `conn()`, `prepare()`, `query_row()`, and
free fn `open_read_only`. `Storage` (`src/storage.rs`) is untouched.

## Layer 2 — shape → method mapping (executor's lookup table)

| call-site shape today | replacement |
|---|---|
| `conn().query_row(sql, p, \|r\| r.get(i))` scalar / COUNT | `query_one(rel, sql, &params, \|r\| Ok(r.get(i)?))` |
| `query_row(...).optional()` | `query_opt(...)` |
| `prepare` + `query_map` + collect (with `filter_map(ok)`) | `query_rows(...)` — errors now propagate |
| `prepare` + `query` + `while rows.next()` fold | `for_each_row(...)`, or `digest_rows` for the XOR fold |
| row → `Vec<Vec<json>>` / `Vec<Vec<String>>` | `query_values(...)` + `SqlVal::to_json` / `to_lossy_string` |
| `conn().execute(sql, [])` | `exec(rel, sql)` |
| `conn().execute(sql, params![..]/array/params_from_iter/&dyn ToSql)` | `exec_params(rel, sql, &[..])` |
| UPDATE/DELETE with manual `IN (?,…)` placeholder building | `exec_in_chunks(rel, \|n\| format!(... holes(n) ...), &head, &keys)` |
| SELECT over 30k-chunk `IN` lists | `query_in_chunks(...)` |
| `conn().execute_batch(script)` DDL | `execute_batch(rel, script)` |
| `execute_batch("BEGIN IMMEDIATE"/"COMMIT"/"ROLLBACK")` | `begin_immediate()` / `commit()` / `rollback()`, or `transact(\|\| ...)` |
| `unchecked_transaction()` + per-row execute loop | `exec_in_chunks(...)` — batched; no manual tx |
| `conn().is_autocommit()` | `is_autocommit()` (unchanged) |
| `let _ = execute("ALTER TABLE … ADD COLUMN …")` | `ensure_column(table, col, ddl)` |
| `PRAGMA x` read | `pragma_i64("x")` |
| `PRAGMA user_version = N` | `exec("_pragma", &format!("PRAGMA user_version = {N}"))` |
| `PRAGMA table_info(t)` / `sqlite_master` scans | `column_exists` / `secondary_indexes` / `schema_objects` |
| multi-row `INSERT … ON CONFLICT` | `upsert_rows(...)` |
| `Statement<'_>`-typed locals | restructure into a closure read; never name `Statement` |
| best-effort telemetry write (`let _ =`) | propagate `?`; if genuinely unactionable, `let _ = db.exec(...)` + `@rusqlite-ok: <why losing it is safe>` |

Param conversion:

| today | seam |
|---|---|
| `params![a, b]` | `&[a.into(), b.into()]` |
| `params_from_iter(xs)` | `xs.iter().map(Into::into).collect::<Vec<SqlVal>>()` |
| `Vec<&dyn ToSql>` built by pushes | `Vec<SqlVal>` pushed directly |
| `stage_id.0.as_slice()`, `base.0.as_slice()` | `SqlVal::from(slice)` → `Blob` |
| serde_json RPC params | `SqlVal::from_json` |
| `Option<T>` | `From<Option<T>>` (None → Null) |

`rel` naming rule: the table the statement chiefly reads or writes (physical
`rel_<name>` form is fine). Multi-table read → the table whose rows are
produced (first `FROM`). DDL script → first object created/dropped. PRAGMA →
`_pragma`. Tx control → the primitives hardcode `_tx`.

## Layer 3 — instance lifetimes

| type | owns | born | dies | rules |
|---|---|---|---|---|
| `Db` | writer `Connection`, N+1 counts, `pending_syms`, write ledger | `db::open` (one per Engine / test) | `Drop` → best-effort `wal_checkpoint(TRUNCATE)` (deliberate swallow, `@rusqlite-ok`) | `&self` methods; `RefCell` counters unchanged |
| `ReadDb` | read-only `Connection` | per `daemon_read` RPC | end of the RPC | never pooled, never crosses threads (`NO_MUTEX` kept) |
| `SqlVal` | owned `String`/`Vec` per param | call site, per statement | consumed at bind | no borrows of caller data; cheap clones accepted |
| tx ownership | no guard type | `begin*` / `transact` | `commit` / `rollback` | who begins, closes; helpers never close an outer tx; `is_autocommit` is the guard |
| prepared statements | — | created inside each seam method | dropped before return | no cache (see todo) |

## Layer 4 — storage layout, sequencing, uniqueness

- **Schema: zero changes.** `SCHEMA_EPOCH` untouched; no table, index, or
  migration is added, dropped, or altered by this arc. A broken intermediate
  db is deleted, not repaired (ephemeral-db ruling).
- **Statement order: preserved 1:1.** Migration is mechanical call
  replacement; no read/write is reordered, no tx boundary moves. The one
  intended restructuring: `drain_effects`' mark-done moves from a per-row
  `unchecked_transaction` loop to chunked `exec_in_chunks` updates; atomicity
  is unchanged because the whole drain already runs inside
  `with_semantic_generation`'s transaction.
- **Uniqueness conditions preserved exactly:** `insert_rows` OR IGNORE PK
  dedup; `upsert_rows` `ON CONFLICT(key_cols)`; `flush_syms` same-flush hash
  collision bail; `sh!` exactly-once = conditional
  `UPDATE … WHERE state='queued'` with rows-affected == 1 (reads
  `exec_params`'s return); `pending_effect.idem_key`.
- **Intended behavior changes (the only ones):**
  1. Row-map errors propagate (was `filter_map(|x| x.ok())`).
  2. `daemon_read`'s `string_rows` prepare failure propagates (was empty
     vec) — unreachable in practice: `query_rel` validates the rel first.
  3. ALTER tolerances become deterministic `ensure_column` checks.
  4. Mark-done batches instead of per-row (above).

## Hard sites → named resolutions

| site | problem | resolution |
|---|---|---|
| Scalar-fn registry (`register_*` in db.rs) | already seam-internal; registered on both open paths | unchanged; `ReadDb::open` calls the same private fns with the read-only `sprf_sym_intern` variant |
| Busy handler (`install_busy_verdict_handler`) | seam-internal | unchanged |
| `open_read_only` → raw `Connection` to daemon_read | biggest read leak | `ReadDb` (Layer 1); `json_rows`/`string_rows`/`cell_to_json` move into db.rs as `query_values` + `SqlVal` converters |
| Pipeline stage builders hold `&'a Connection` across a batch (`SourceStage`, `StageWriter`, `FullSourceStageBuilder`, `derive_ready`, `verify_ready`, `PreparedStage::discard`) | raw conn stored in structs | retype fields/params to `&'a Db` (lifetimes unchanged); stage SQL becomes `exec`/`exec_params`/`execute_batch` with `SqlVal::Blob` ids; callers pass `&engine.db`; `source_stage_tests.rs`'s `file_connection()` opens a `Db` via `db::open` instead of `Connection::open` |
| `effect.rs:841` `unchecked_transaction` + per-row UPDATE | N+1 inside a manual tx | `exec_in_chunks` `UPDATE … WHERE id IN (…)`; manual tx deleted (enclosing generation tx covers atomicity) |
| `apply.rs` `with_semantic_generation` | BEGIN/COMMIT/ROLLBACK via `conn()` | named primitives; `catch_unwind` kept; rollback-on-error stays `let _ =` with `@rusqlite-ok` (a rollback failure is unactionable; the original error wins) |
| `meta.rs` ALTER tolerance swallows (7) | deliberate duplicate-column swallows | `ensure_column` |
| `meta.rs` `digest_of_query` | row fold over `rusqlite::types::Value` | `Db::digest_rows` (byte-identical) |
| `meta.rs` save/delete/load `_reldigest` helpers | `params_from_iter`, manual holes | `upsert_rows` / `exec_in_chunks` / `query_rows` |
| `declare.rs` `OptionalExtension` | `use rusqlite::OptionalExtension` | `query_opt` |
| `daemon.rs` / `invlog.rs` / `jobq` / `tick.rs` swallows | best-effort telemetry / checkpoint | propagate `?` where the caller returns `Result`; otherwise `let _ =` + `@rusqlite-ok: <why losing it is safe>` |

## Execution shape

Two parallel worktree agents; the seam-API commit lands FIRST, both branch
from it.

### Commit 0 — seam API lands (owner: Agent A)

- `src/db.rs`: everything in Layer 1 (`SqlVal`, `sql_err`, all read/write/
  introspection/tx methods, `ReadDb`); `exec`/`execute_batch` gain the `rel`
  param and their few existing call sites are fixed; db.rs's own
  `#[cfg(test)]` module migrates off `conn()`; the 4 deliberate seam-internal
  swallows get `@rusqlite-ok` comments and the `src/db.rs` swallow baseline
  row is deleted.
- New db.rs unit tests: one per new method (round-trip), plus an
  error-context test asserting a failing statement's error string contains
  the table name AND the statement head.
- Gate: `cargo check`; `cargo test --lib`; `dl .dl/no-new-rusqlite.dl --check`
  shows no count rising.

### File partition (whole files, including each file's `#[cfg(test)]` module)

Counts are `rusqlite / conn / swallow` baseline rows. Agent A owns
`src/**` app files; Agent B owns `tests/it/**` and the three listed
`*_tests.rs` files. Nothing else exists: if the rail flags a file not listed
here, the batch owner of its directory takes it.

| batch | files (counts) | sites |
|---|---|---|
| A1 read path | daemon_read.rs 21/3/0 · engine/rpc.rs 23/14/0 · engine/query.rs 4/2/0 | 67 |
| A2 engine meta | engine/meta.rs 12/60/7 | 79 |
| A3 derive+declare | engine/derive.rs 8/25/0 · engine/declare.rs 4/21/0 · engine/decls.rs 0/4/0 | 62 |
| A4 effects+jobs | effect.rs 16/18/0 · jobq/mod.rs 2/8/2 · daemon.rs 0/0/2 | 48 |
| A5 pipeline | engine/pipeline/apply.rs 1/14/3 · full_sources.rs 1/2/0 · full_sources_tests.rs 0/18/0 · source_stage.rs 6/0/0 · source_stage_read.rs 1/0/0 · source_stage_tests.rs 0/0/0* · engine/cold_stage.rs 2/12/0 · engine/source_prepare.rs 0/9/0 | 69 |
| A6 anchors+lens | anchor.rs 15/2/0 · engine/lens.rs 12/10/0 · engine/reconcile.rs 8/4/0 · engine/path_reconcile.rs 0/2/0 · engine/gen.rs 1/1/0 · engine/generation.rs 7/0/0 · engine/repo.rs 4/9/0 | 75 |
| A7 engine core | engine/family/mod.rs 1/6/0 · engine/mod.rs 6/2/0 · engine/ownership.rs 2/0/0 · engine/tick.rs 0/7/1 · engine/staged_delta/mod.rs 4/0/0 · engine/staged_delta/sql.rs 3/0/0 · engine/deltaflow.rs 4/0/0 | 36 |
| A8 rels+extract | engine/extract/call.rs 0/1/0 · extract/doc.rs 0/1/0 · extract/mod.rs 0/17/0 · extract/node.rs 0/2/0 · extract/text.rs 0/3/0 · rels/analysis.rs 0/7/0 · rels/embed.rs 2/3/0 · rels/env.rs 0/1/0 · rels/filelines.rs 0/2/0 · rels/git.rs 0/7/0 · rels/perf.rs 0/5/0 · rels/propose.rs 0/2/0 · rels/querylog.rs 0/2/0 · rels/scip.rs 0/1/0 · invlog.rs 2/0/2 · agent.rs 2/0/0 | 62 |
| B1 it suite | tests/it/: checkout_sweep 3 · storage_diet_norm 5 · ghcacher_parity 4 · config_repos 1 · gh_cache 4 · spine_meta 1 · family_op_raw_sql_audit 5 · halt_bfs 4 · temporal_async 6 (rail-untracked counts) | ~33 |
| B2 src tests | engine/extract/call_render_tests.rs 2/20/0 · engine/staged_delta/tests.rs 1/0/0 · jobq/tests.rs 0/5/1 | 29 |

\* `source_stage_tests.rs` holds raw `Connection`s that the rail's regex
cannot see (imported via `use super::*`) — migrate it anyway; the goal is
zero rusqlite API usage, the rail is the floor.

A5 owns `full_sources_tests.rs` and `source_stage_tests.rs` (not B) because
the `&Connection` → `&Db` retype breaks them in A's own tree; they land
migrated in the same commit.

### Per-batch verification gate (both agents, every batch)

1. `cargo check` clean.
2. `cargo test` — no new failures (flake policy per `scripts/verify.sh`:
   re-run the failure solo; passing solo = flake, report it).
3. `dl .dl/no-new-rusqlite.dl --check` — owned files' counts dropped.
4. The SAME commit lowers (or deletes, at 0) the owned files' baseline rows
   in `.dl/no-new-rusqlite.dl`. Never raise a row.
5. Commit message: `seam-migrate(<batch>): <files> (r:-N c:-N s:-N)`.
   Batches are sized (29–79 sites) to stay reviewable; do not merge batches.

### Merge order

1. Commit 0 → main.
2. A1…A8 land sequentially on main (one worktree, one agent).
3. B1 (tests/it only) may land any time after commit 0 — disjoint files, no
   `.dl` rows touched.
4. After A5 lands, B rebases onto main tip; B2 lands.
5. Final burn-down commit (Agent A): delete `Db::conn`/`prepare`/`query_row`
   and `open_read_only`; verify every baseline row in all three rels is 0 and
   delete all rows; rail runs with empty baseline rels and stays green.

### Collision rules

- A never touches `tests/` or B2's three files; B never touches `src/` app
  files, `src/db.rs`, or `src/storage*`.
- `.dl/no-new-rusqlite.dl`: each agent lowers only its owned files' rows
  (disjoint lines; B1 touches none).
- `engine/db` field stays `pub(crate)`; no new visibility is introduced.

## Verification

- New db.rs unit tests from commit 0, including the error-context assertion
  (table name + statement head present in the failure string) — the
  fail-pre-fix for the incident class.
- `cargo test` green at every commit; full suite (not just `--lib`) at least
  once per agent before merging.
- `dl .dl/no-new-rusqlite.dl --check` per batch; final run with empty
  baseline rels must be green.
- Terminal grep proof (must return nothing):
  `rg -n 'rusqlite|\.conn\(\)' src tests --glob '!src/db.rs' --glob '!src/storage.rs' --glob '!src/storage/**'`
- `scripts/verify.sh` full pass before the final burn-down commit.
- Perf sanity: one `--profile` `--check` run on this repo before commit 0 and
  after the final commit; per-tick statement counts should match within
  noise. A chunk-loop regression from prepare-per-call is the accepted risk
  (todo below).

<!-- todo(perf): after the seam migration, profile a chunk-loop-heavy --check run; if prepare-per-call regressed a hot loop, add a seam-internal statement cache keyed by SQL -->
<!-- todo(feature): extend .dl/no-new-rusqlite.dl scans from src/**/*.rs to tests/**/*.rs so the rail covers the it suite -->
<!-- todo(decision): the pre-existing Storage trait (src/storage.rs) predates the struct ruling; collapse it into Db's inherent API or keep it for CallStore — decided outside this arc -->
<!-- todo(docs): src/db.rs grows past 1500 lines under this plan; it is already in scripts/filesize-allow.txt, but a future split into src/db/ requires extending rusqlite_seam in .dl/no-new-rusqlite.dl -->

## Staffing

- Two sonnet-tier agents, two git worktrees branched from commit 0. Base SHA
  at plan time: `e8d46d42` (rebase if main has moved; the working tree's
  unrelated README/docs edits stay out of both worktrees).
- **Agent A** (app code): worktree `.worktrees/seam-app`. Owns commit 0,
  batches A1–A8 (~498 sites), and the final burn-down commit.
- **Agent B** (tests): worktree `.worktrees/seam-tests`. Owns B1 (~33 sites,
  starts right after commit 0) and B2 (29 sites, after rebasing onto A5).
- Suite budget: `cargo check` + `cargo test` per batch; rail run per batch;
  `scripts/verify.sh` once per agent before its final merge.
