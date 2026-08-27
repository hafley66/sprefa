//! sprefa-store: the v6 storage spine as a standalone crate.
//!
//! - `spine`   — the CLOSED data model (9 tables), DDL derived from the entities.
//! - `strings` — resident interning (lasso), the v5 string subsystem on blast.
//! - `transforms` — pure pre-storage transforms, each replacing a named v5 hack.
//! - `rels`    — the OPEN core: mint new rel tables at runtime.
//! - `unfuck_sqlite` — every SQLite-ism, behind a trait.
//!
//! `Store` is the one object that speaks the ORM; callers get ids/rows, never a
//! SeaORM type or SQL text. Writes are batched (the N+1 law) and FK-ordered.

// The v6 store is FIVE modules: lib (Store + string/relstore helpers), spine
// (data model), engine (the one cascade), measure (golden harness), oracle
// (dd/salsa/rust correctness oracles). `strings` (Interner) and `relstore` are
// inlined at the bottom of this file. Back-compat re-exports below keep every
// existing `crate::cascade`/`crate::unfuck_sqlite`/… path resolving, so no
// test or example needs a path edit after the fold.
pub mod spine;
pub mod engine;
pub mod measure;
pub mod oracle;
pub mod algo;

pub use engine::{cascade, reach, reconcile, temporal};
pub use measure::{benchgraph, memcap};
pub use spine::{rels, transforms, unfuck_sqlite};

use sea_orm::sea_query::OnConflict;
use sea_orm::ActiveValue::{NotSet, Set};
use sea_orm::{
    ColumnTrait, ConnectionTrait, Database, DatabaseBackend, DatabaseConnection, DbErr,
    EntityTrait, QueryFilter, QueryOrder, Statement,
};
use std::collections::HashMap;
use std::sync::Mutex;

use spine::{edge, file_bytes, files, node, repo_revs, repos, revs_files, roots};
use strings::Interner;
use unfuck_sqlite::OPEN_PRAGMAS;

/// Widest table is `node` at 7 columns; 100 rows/statement keeps bound params
/// under SQLite's conservative 999 ceiling on any bundled build.
const CHUNK_ROWS: usize = 100;

/// Global, resettable count of SQL statements issued by the Store. This is the
/// N+1 tripwire: a golden test resets it, runs a batch of N rows, and asserts
/// the count stayed O(N / CHUNK_ROWS), never O(N). It is process-global and
/// lock-free (a relaxed atomic) so it never contends with anything.
pub mod stmt_counter {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNT: AtomicU64 = AtomicU64::new(0);

    /// Bump the count by one issued statement.
    pub fn incr() {
        COUNT.fetch_add(1, Ordering::Relaxed);
    }

    /// Current statement count.
    pub fn get() -> u64 {
        COUNT.load(Ordering::Relaxed)
    }

    /// Reset to zero (call before a measured batch).
    pub fn reset() {
        COUNT.store(0, Ordering::Relaxed);
    }
}

pub struct NodeRow {
    pub node_id: i64,
    pub family: i32,
    pub file_id: i64,
    pub byte_start: i64,
    pub byte_len: i64,
    pub kind: i32,
    pub name_id: Option<i64>,
}

pub struct EdgeRow {
    pub family: i32,
    pub src_id: i64,
    pub dst_id: i64,
    pub kind: i32,
}

pub struct SpanRow {
    pub file_id: i64,
    pub start: i64,
    pub end: i64,
    pub string_id: Option<i64>,
}

pub struct Store {
    db: DatabaseConnection,
    interner: Mutex<Interner>,
}

impl Store {
    /// Open a store, apply pragmas, create the spine, hydrate the resident
    /// interner from the durable mirror.
    pub async fn open(url: &str) -> Result<Self, DbErr> {
        let db = Database::connect(url).await?;
        db.execute_unprepared(OPEN_PRAGMAS).await?;
        spine::create_all_tables(&db).await?;
        let mut interner = Interner::new();
        let rows = <spine::strings::Entity as FindOrdered>::find_ordered(&db).await?;
        for (id, content) in rows {
            interner.load_row(id, &content);
        }
        Ok(Self {
            db,
            interner: Mutex::new(interner),
        })
    }

    pub fn db(&self) -> &DatabaseConnection {
        &self.db
    }

    // ---- strings: resident intern + batched durable flush ------------------

    /// Intern text -> dense `string_id`, resident. Does not touch the DB; call
    /// `flush_strings` to persist the batch before any row FKs into `strings`.
    /// The interner lock is taken and released synchronously here (no `.await`
    /// under it), so it never contends with any DB round-trip.
    pub fn intern(&self, text: &str) -> i64 {
        self.interner.lock().unwrap().intern(text)
    }

    pub fn resolve(&self, id: i64) -> Option<String> {
        self.interner
            .lock()
            .unwrap()
            .resolve(id)
            .map(|s| s.to_string())
    }

    /// Persist every string interned since the last flush in ONE batched insert
    /// (chunked). Returns the number of new rows written.
    pub async fn flush_strings(&self) -> Result<usize, DbErr> {
        // Drain under the lock, then DROP it before any `.await`. The lock is
        // never held across a DB round-trip, so the writer task and any reader
        // never block on it.
        let dirty = {
            let mut guard = self.interner.lock().unwrap();
            guard.take_dirty()
        };
        if dirty.is_empty() {
            return Ok(0);
        }
        let n = dirty.len();
        for chunk in dirty.chunks(CHUNK_ROWS) {
            let models: Vec<spine::strings::ActiveModel> = chunk
                .iter()
                .map(|(id, content)| spine::strings::ActiveModel {
                    string_id: Set(*id),
                    content: Set(content.clone()),
                })
                .collect();
            stmt_counter::incr();
            spine::strings::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(spine::strings::Column::StringId)
                        .do_nothing()
                        .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(n)
    }

    // ---- dimensions --------------------------------------------------------

    pub async fn repo_upsert(&self, slug: &str, root: &str, url: &str) -> Result<i64, DbErr> {
        stmt_counter::incr();
        if let Some(m) = repos::Entity::find()
            .filter(repos::Column::Slug.eq(slug))
            .one(&self.db)
            .await?
        {
            return Ok(m.repo_id);
        }
        stmt_counter::incr();
        let res = repos::Entity::insert(repos::ActiveModel {
            repo_id: NotSet,
            slug: Set(slug.to_string()),
            root: Set(root.to_string()),
            url: Set(url.to_string()),
        })
        .exec(&self.db)
        .await?;
        Ok(res.last_insert_id)
    }

    pub async fn root_insert(&self, repo_id: i64, path_string_id: i64) -> Result<i64, DbErr> {
        stmt_counter::incr();
        let res = roots::Entity::insert(roots::ActiveModel {
            root_id: NotSet,
            repo_id: Set(repo_id),
            path_string_id: Set(path_string_id),
        })
        .exec(&self.db)
        .await?;
        Ok(res.last_insert_id)
    }

    /// A committed rev (shared across roots that have this sha). Find-or-insert
    /// by `(repo_id, git_sha)`.
    pub async fn rev_committed(&self, repo_id: i64, git_sha: [u8; 20]) -> Result<i64, DbErr> {
        let sha = git_sha.to_vec();
        stmt_counter::incr();
        if let Some(m) = repo_revs::Entity::find()
            .filter(repo_revs::Column::RepoId.eq(repo_id))
            .filter(repo_revs::Column::GitSha.eq(sha.clone()))
            .one(&self.db)
            .await?
        {
            return Ok(m.rev_id);
        }
        stmt_counter::incr();
        let res = repo_revs::Entity::insert(repo_revs::ActiveModel {
            rev_id: NotSet,
            repo_id: Set(repo_id),
            kind: Set(spine::RevKind::Committed.as_i32()),
            git_sha: Set(Some(sha)),
            root_id: Set(None),
            base_rev_id: Set(None),
        })
        .exec(&self.db)
        .await?;
        Ok(res.last_insert_id)
    }

    /// The WORK rev of a root (its uncommitted working tree, no sha), diverging
    /// from committed `base_rev_id` (the root's HEAD). One per root, enforced by
    /// the partial unique index.
    pub async fn rev_work(
        &self,
        repo_id: i64,
        root_id: i64,
        base_rev_id: i64,
    ) -> Result<i64, DbErr> {
        stmt_counter::incr();
        if let Some(m) = repo_revs::Entity::find()
            .filter(repo_revs::Column::RootId.eq(root_id))
            .filter(repo_revs::Column::Kind.eq(spine::RevKind::Work.as_i32()))
            .one(&self.db)
            .await?
        {
            return Ok(m.rev_id);
        }
        stmt_counter::incr();
        let res = repo_revs::Entity::insert(repo_revs::ActiveModel {
            rev_id: NotSet,
            repo_id: Set(repo_id),
            kind: Set(spine::RevKind::Work.as_i32()),
            git_sha: Set(None),
            root_id: Set(Some(root_id)),
            base_rev_id: Set(Some(base_rev_id)),
        })
        .exec(&self.db)
        .await?;
        Ok(res.last_insert_id)
    }

    // ---- files (content, dedup by hash) ------------------------------------

    /// Batch-insert content rows, dedup on `content_hash` (identical bytes = one
    /// row). Idempotent.
    pub async fn files_insert_batch(
        &self,
        rows: &[([u8; 16], i64, i64)],
    ) -> Result<(), DbErr> {
        for chunk in rows.chunks(CHUNK_ROWS) {
            let models: Vec<files::ActiveModel> = chunk
                .iter()
                .map(|(hash, size, lines)| files::ActiveModel {
                    file_id: NotSet,
                    content_hash: Set(hash.to_vec()),
                    size: Set(*size),
                    lines: Set(*lines),
                })
                .collect();
            stmt_counter::incr();
            files::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(files::Column::ContentHash)
                        .do_nothing()
                        .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }

    pub async fn file_id_of(&self, content_hash: [u8; 16]) -> Result<Option<i64>, DbErr> {
        stmt_counter::incr();
        Ok(files::Entity::find()
            .filter(files::Column::ContentHash.eq(content_hash.to_vec()))
            .one(&self.db)
            .await?
            .map(|m| m.file_id))
    }

    /// Batch `content_hash -> file_id` resolution: ONE query per CHUNK_ROWS
    /// hashes, never one per file. This is the no-N+1 replacement for calling
    /// `file_id_of` in a loop during ingestion.
    pub async fn file_ids_by_hashes(
        &self,
        hashes: &[[u8; 16]],
    ) -> Result<HashMap<[u8; 16], i64>, DbErr> {
        let mut map = HashMap::with_capacity(hashes.len());
        for chunk in hashes.chunks(CHUNK_ROWS) {
            let keys: Vec<Vec<u8>> = chunk.iter().map(|h| h.to_vec()).collect();
            stmt_counter::incr();
            let rows = files::Entity::find()
                .filter(files::Column::ContentHash.is_in(keys))
                .all(&self.db)
                .await?;
            for m in rows {
                let mut key = [0u8; 16];
                key.copy_from_slice(&m.content_hash);
                map.insert(key, m.file_id);
            }
        }
        Ok(map)
    }

    /// Paths in a WORK rev whose content differs from its base HEAD — i.e. the
    /// files with unstaged changes. ONE join, never a per-file compare. This is
    /// v1's `<sha>+` dirty flag done relationally: the WORK rev's `base_rev_id`
    /// FK + a self-join on `revs_files`.
    pub async fn unstaged_path_ids(&self, work_rev: i64) -> Result<Vec<i64>, DbErr> {
        let sql = format!(
            "SELECT w.path_string_id \
             FROM revs_files w \
             JOIN repo_revs r ON r.rev_id = w.rev_id \
             LEFT JOIN revs_files h \
               ON h.rev_id = r.base_rev_id AND h.path_string_id = w.path_string_id \
             WHERE w.rev_id = {work_rev} \
               AND (h.file_id IS NULL OR h.file_id <> w.file_id)"
        );
        stmt_counter::incr();
        let rows = self
            .db
            .query_all_raw(Statement::from_string(DatabaseBackend::Sqlite, sql))
            .await?;
        rows.into_iter()
            .map(|r| r.try_get_by_index::<i64>(0))
            .collect::<Result<Vec<_>, _>>()
            .map_err(DbErr::from)
    }

    // ---- junction: place content at (rev, path) ----------------------------

    pub async fn place_files_batch(
        &self,
        rows: &[(i64, i64, i64)], // (rev_id, path_string_id, file_id)
    ) -> Result<(), DbErr> {
        for chunk in rows.chunks(CHUNK_ROWS) {
            let models: Vec<revs_files::ActiveModel> = chunk
                .iter()
                .map(|(rev_id, path, file_id)| revs_files::ActiveModel {
                    rev_id: Set(*rev_id),
                    path_string_id: Set(*path),
                    file_id: Set(*file_id),
                })
                .collect();
            stmt_counter::incr();
            revs_files::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        revs_files::Column::RevId,
                        revs_files::Column::PathStringId,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }

    // ---- unified graph -----------------------------------------------------

    pub async fn nodes_insert_batch(&self, rows: &[NodeRow]) -> Result<(), DbErr> {
        for chunk in rows.chunks(CHUNK_ROWS) {
            let models: Vec<node::ActiveModel> = chunk
                .iter()
                .map(|r| node::ActiveModel {
                    node_id: Set(r.node_id),
                    family: Set(r.family),
                    file_id: Set(r.file_id),
                    byte_start: Set(r.byte_start),
                    byte_len: Set(r.byte_len),
                    kind: Set(r.kind),
                    name_id: Set(r.name_id),
                })
                .collect();
            stmt_counter::incr();
            node::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::column(node::Column::NodeId)
                        .do_nothing()
                        .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }

    pub async fn edges_insert_batch(&self, rows: &[EdgeRow]) -> Result<(), DbErr> {
        for chunk in rows.chunks(CHUNK_ROWS) {
            let models: Vec<edge::ActiveModel> = chunk
                .iter()
                .map(|r| edge::ActiveModel {
                    family: Set(r.family),
                    src_id: Set(r.src_id),
                    dst_id: Set(r.dst_id),
                    kind: Set(r.kind),
                })
                .collect();
            stmt_counter::incr();
            edge::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        edge::Column::Family,
                        edge::Column::SrcId,
                        edge::Column::DstId,
                        edge::Column::Kind,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }

    pub async fn spans_insert_batch(&self, rows: &[SpanRow]) -> Result<(), DbErr> {
        for chunk in rows.chunks(CHUNK_ROWS) {
            let models: Vec<file_bytes::ActiveModel> = chunk
                .iter()
                .map(|r| file_bytes::ActiveModel {
                    file_id: Set(r.file_id),
                    start: Set(r.start),
                    end: Set(r.end),
                    string_id: Set(r.string_id),
                })
                .collect();
            stmt_counter::incr();
            file_bytes::Entity::insert_many(models)
                .on_conflict(
                    OnConflict::columns([
                        file_bytes::Column::FileId,
                        file_bytes::Column::Start,
                        file_bytes::Column::End,
                    ])
                    .do_nothing()
                    .to_owned(),
                )
                .exec_without_returning(&self.db)
                .await?;
        }
        Ok(())
    }
}

/// Small extension so `open` can read the mirror in id order without leaking
/// query types into `open`'s body.
trait FindOrdered {
    async fn find_ordered(db: &DatabaseConnection) -> Result<Vec<(i64, String)>, DbErr>;
}

impl FindOrdered for spine::strings::Entity {
    async fn find_ordered(db: &DatabaseConnection) -> Result<Vec<(i64, String)>, DbErr> {
        Ok(spine::strings::Entity::find()
            .order_by_asc(spine::strings::Column::StringId)
            .all(db)
            .await?
            .into_iter()
            .map(|m| (m.string_id, m.content))
            .collect())
    }
}

// ---- folded from strings.rs (Interner) / relstore.rs ----
pub mod strings {
//! Resident string interning, on blast. THE v5 pain point, replaced by a
//! library (lasso). What v5 did for its string table, itemized so it never
//! comes back:
//!
//!   - `StringId = hash64(text)` (spine.rs:52-57): ids were 64-bit content
//!     hashes. 8 flat bytes each, defeating SQLite varint, in the `_strings`
//!     table AND in every rel index that referenced a `sym` column.
//!   - `SymAlloc` (db.rs): a bespoke in-memory hash->dense-id allocator with
//!     load-once / single-writer / persist-at-flush. That is exactly what an
//!     interner IS — reimplemented here by `lasso::Rodeo`.
//!   - `persisted_strings` + `inflight_strings`: two `RefCell<HashSet<i64>>`
//!     tracking which hashes had already been committed, because re-interning
//!     an unchanged corpus offered 1,207,064 rows to accept 146 (db.rs:97-124).
//!     That whole dance existed only because a hash id had no cheap "have I seen
//!     this string" — an interner answers that in O(1) by construction.
//!   - `flush_syms` collision guard: needed because two different texts could
//!     share one 64-bit hash. Dense sequential assignment cannot collide, so the
//!     guard is deleted outright.
//!   - `salt_rev` / `\u{1}` concatenation: rev/repo smuggled into id strings so
//!     hashed coordinates stayed disjoint across revs. Gone — a rev is a column
//!     (repo_revs), a node is content-scoped, nothing salts a string.
//!
//! v6: `lasso::Rodeo` is the resident arena AND the id authority. `string_id` is
//! the dense `Spur` index (0-based, contiguous). The `strings` table is the
//! durable MIRROR of the arena, never the source. New interns queue in `dirty`
//! for ONE batched insert (the N+1 law). Freeze to `RodeoReader` when a
//! read-only resident view with no lock is wanted.

use lasso::{Key, Rodeo, Spur};

/// The resident interner. Owns the string arena and assigns dense ids.
pub struct Interner {
    rodeo: Rodeo,
    dirty: Vec<(i64, String)>,
}

impl Interner {
    pub fn new() -> Self {
        Self {
            rodeo: Rodeo::default(),
            dirty: Vec::new(),
        }
    }

    /// Intern `text`, returning its dense `string_id`. Queues `(id, text)` for
    /// the durable flush the first time a string is seen; a repeat returns the
    /// same id and queues nothing.
    pub fn intern(&mut self, text: &str) -> i64 {
        let seen = self.rodeo.get(text).is_some();
        let spur = self.rodeo.get_or_intern(text);
        let id = spur.into_usize() as i64;
        if !seen {
            self.dirty.push((id, text.to_string()));
        }
        id
    }

    /// `string_id -> text`, straight from the resident arena, no DB round-trip.
    pub fn resolve(&self, id: i64) -> Option<&str> {
        let key = Spur::try_from_usize(usize::try_from(id).ok()?)?;
        self.rodeo.try_resolve(&key)
    }

    /// Rebuild the arena from the durable mirror on open. Rows MUST arrive in
    /// ascending `string_id` order so the reconstructed `Spur` equals the stored
    /// id — asserted, because a mismatch means the mirror and the arena disagree
    /// on identity, which would silently corrupt every FK into `strings`.
    pub fn load_row(&mut self, id: i64, content: &str) {
        let spur = self.rodeo.get_or_intern(content);
        let got = spur.into_usize() as i64;
        assert_eq!(
            got, id,
            "interner reload out of order: got id {got} for {content:?}, expected {id}"
        );
    }

    /// Drain the queued new interns for a batched `strings` insert.
    pub fn take_dirty(&mut self) -> Vec<(i64, String)> {
        std::mem::take(&mut self.dirty)
    }

    pub fn len(&self) -> usize {
        self.rodeo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rodeo.is_empty()
    }
}

impl Default for Interner {
    fn default() -> Self {
        Self::new()
    }
}

}
pub mod relstore {
//! RelStore — the generic incremental relation store. Lifts the cascade + reconcile
//! off the bespoke `cx_`/`rx_` scaffolding into one handle keyed by DENSE `(rel, row)`
//! integer ids (E1: `key = rel*KEY_STRIDE + row`, a rowid-clustered table). One store
//! holds ANY number of relations; `rel` is the relation discriminator, `row` the tuple.
//!
//! Two planes, both generic:
//!   FACT (Z-set):  add_rows / add_deps / assert / retract / retract_dred / alive
//!   CONTROL (salsa-in-sql): seed_memo / mark_changed / dirty / verify
//!
//! The `cx_*` / `rx_*` tables are just the default on-disk impl; callers speak only in
//! `(rel, row)` pairs. This is what the harness measures now, instead of a bespoke copy.

use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

use crate::{cascade, reconcile};

pub use cascade::{key, KEY_STRIDE};

/// The namespace for one graph store: every persistent table, index, and TEMP
/// working-table name, built from a prefix. `GraphNs::default()` (empty prefix) is
/// the live `cx_`/`rx_` set; `GraphNs::new("b_")` is an independent store in the
/// same db.
///
/// PREFIX, not schema-qualify, is the namespace mechanism: SQLite TEMP working
/// tables live in `temp.` and CANNOT be qualified to an ATTACH'd schema, so prefix
/// is the only namespace that covers the working set. Epic 2 threaded this through
/// cascade/reconcile/reach; the retired `Layout` measurement knob is gone (the
/// frozen collapse-vs-split evidence lives in `measure::stamp_collapsed_evidence`).
#[derive(Clone, Debug)]
pub struct GraphNs {
    pub row: String,          // {p}cx_row    — cascade Z-set nodes
    pub dep: String,          // {p}cx_dep    — cascade edges
    pub memo: String,         // {p}rx_memo   — reconcile digests
    pub rdep: String,         // {p}rx_dep    — reconcile edges
    pub ix_dep_child: String, // {p}ix_cx_dep_child — reverse-traversal index
    pub ix_rdep_read: String, // {p}ix_rx_read
    pub frontier: String,     // TEMP working set ({p}cx_frontier, ...)
    pub next: String,
    pub hits: String,
    pub cone: String,
    pub scc_scope: String,
    pub scc_frontier: String,
    pub scc_next: String,
    pub scc_live: String,
}

impl GraphNs {
    /// `prefix` is prepended verbatim to every base name; pass `"b_"` for a
    /// namespace, `""` for the live default (include any separator yourself).
    pub fn new(prefix: &str) -> Self {
        Self {
            row: format!("{prefix}cx_row"),
            dep: format!("{prefix}cx_dep"),
            memo: format!("{prefix}rx_memo"),
            rdep: format!("{prefix}rx_dep"),
            ix_dep_child: format!("{prefix}ix_cx_dep_child"),
            ix_rdep_read: format!("{prefix}ix_rx_read"),
            frontier: format!("{prefix}cx_frontier"),
            next: format!("{prefix}cx_next"),
            hits: format!("{prefix}cx_hits"),
            cone: format!("{prefix}cx_cone"),
            scc_scope: format!("{prefix}cx_scc_scope"),
            scc_frontier: format!("{prefix}cx_scc_frontier"),
            scc_next: format!("{prefix}cx_scc_next"),
            scc_live: format!("{prefix}cx_scc_live"),
        }
    }
}

impl Default for GraphNs {
    fn default() -> Self {
        Self::new("")
    }
}

/// Stamp the split two-plane schema (cascade `cx_*` + reconcile `rx_*` + TEMP
/// working set + indexes) under the namespace `ns` onto `db` (pragmas first).
///
/// `ns = GraphNs::default()` (empty prefix) reproduces the live `cx_`/`rx_` set
/// byte-for-byte, so the 1.1 schema-equality golden stays green; a custom prefix
/// yields an independent store in the same db. The collapsed shape (g_node/g_edge)
/// is measurement evidence only and lives in [`crate::measure::stamp_collapsed_evidence`].
///
/// Build-vs-buy: hand-rolled DDL. The lab need is ephemeral schema for a fresh
/// temp db per run (create_schema already `format!`s); sea-query / refinery /
/// sqlx-migrations version a LIVE db, which is not this.
pub async fn stamp(db: &DatabaseConnection, ns: &GraphNs) -> Result<(), DbErr> {
    db.execute_unprepared(crate::unfuck_sqlite::OPEN_PRAGMAS).await?;
    cascade::create_schema(db, ns).await?;
    reconcile::create_schema(db, ns).await?;
    Ok(())
}

pub struct RelStore {
    db: DatabaseConnection,
    ns: GraphNs,
}

impl RelStore {
    /// Open (or create) a store at `db` stamped for namespace `ns`. The namespace
    /// lives here; everything above it (RelStore API, measure harness) is fixed.
    pub async fn attach_with(db: DatabaseConnection, ns: GraphNs) -> Result<Self, DbErr> {
        stamp(&db, &ns).await?;
        Ok(Self { db, ns })
    }

    /// Open (or create) a store at `db` with the store's tuning + both schemas.
    /// The status quo: the default namespace (cx_ + rx_). Delegates to [`attach_with`].
    pub async fn attach(db: DatabaseConnection) -> Result<Self, DbErr> {
        Self::attach_with(db, GraphNs::default()).await
    }

    pub fn conn(&self) -> &DatabaseConnection {
        &self.db
    }

    /// The namespace this store's `cx_*`/`rx_*` tables live under.
    pub fn ns(&self) -> &GraphNs {
        &self.ns
    }

    // ---- FACT plane (generic Z-set over (rel,row)) ----------------------------

    /// Insert `(rel, row, weight)` tuples.
    pub async fn add_rows(&self, rows: &[(i64, i64, i64)]) -> Result<(), DbErr> {
        cascade::insert_rows(&self.db, &self.ns, rows).await
    }
    /// Insert dependency edges `(parent_rel, parent_row, child_rel, child_row)`.
    pub async fn add_deps(&self, edges: &[(i64, i64, i64, i64)]) -> Result<(), DbErr> {
        cascade::insert_deps(&self.db, &self.ns, edges).await
    }
    /// Forward add: propagate aliveness from `seeds`. Returns rounds.
    pub async fn assert(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::assert(&self.db, &self.ns, seeds).await
    }
    /// Counting retraction (fast, correct on ACYCLIC ref-count graphs). Returns rounds.
    pub async fn retract(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract(&self.db, &self.ns, seeds).await
    }
    /// Counting retraction with an on-disk SCC-scoped nested fixpoint. Returns rounds.
    pub async fn retract_scc(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_scc(&self.db, &self.ns, seeds).await
    }
    /// Cycle-safe retraction (Delete-and-Rederive), Rust-driven round loop. Returns rounds.
    pub async fn retract_dred(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_dred(&self.db, &self.ns, seeds).await
    }
    /// Cycle-safe retraction as two recursive CTEs (whole traversal in SQLite's C
    /// engine, no per-round round-trip). Same result as `retract_dred`; use at scale.
    pub async fn retract_dred_cte(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_dred_cte(&self.db, &self.ns, seeds).await
    }
    pub async fn retract_signed_delta_cte(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_signed_delta_cte(&self.db, &self.ns, seeds).await
    }
    pub async fn retract_delta_fold(&self, seeds: &[(i64, i64)]) -> Result<u64, DbErr> {
        cascade::retract_delta_fold(&self.db, &self.ns, seeds).await
    }
    /// Count live rows (weight > 0) across all relations.
    pub async fn alive(&self) -> Result<i64, DbErr> {
        use sea_orm::{DatabaseBackend, Statement};
        Ok(self
            .db
            .query_one_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                format!("SELECT count(*) FROM {} WHERE weight>0", self.ns.row),
            ))
            .await?
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
            .unwrap_or(0))
    }

    /// The live-row survivor SET as sorted encoded keys (`key = rel*KEY_STRIDE + row`).
    /// This is the answer bytes the head-to-head diffs against the oracle and dd. `key`
    /// IS the rowid and is stored ordered, so `ORDER BY key` is a no-sort ordered scan.
    pub async fn alive_keys(&self) -> Result<Vec<i64>, DbErr> {
        use sea_orm::{DatabaseBackend, Statement};
        Ok(self
            .db
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                format!("SELECT key FROM {} WHERE weight>0 ORDER BY key", self.ns.row),
            ))
            .await?
            .iter()
            .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
            .collect())
    }

    // ---- CONTROL plane (salsa-in-sql over (rel,row) memos) --------------------

    pub async fn seed_memo(&self, rel: i64, row: i64, digest: i64, deps: &[(i64, i64)], rev: i64) -> Result<(), DbErr> {
        let dep_keys: Vec<i64> = deps.iter().map(|&(r, w)| key(r, w)).collect();
        reconcile::seed(&self.db, &self.ns, key(rel, row), digest, &dep_keys, rev).await
    }
    pub async fn mark_changed(&self, cells: &[(i64, i64)], rev: i64) -> Result<(), DbErr> {
        let ks: Vec<i64> = cells.iter().map(|&(r, w)| key(r, w)).collect();
        reconcile::mark_changed(&self.db, &self.ns, &ks, rev).await
    }
    /// The stale frontier as `(rel, row)` pairs.
    pub async fn dirty(&self) -> Result<Vec<(i64, i64)>, DbErr> {
        Ok(reconcile::dirty(&self.db, &self.ns)
            .await?
            .into_iter()
            .map(|k| (k / KEY_STRIDE, k % KEY_STRIDE))
            .collect())
    }
    /// Record a recomputed rel's digest; returns whether it moved (early cutoff).
    pub async fn verify(&self, rel: i64, row: i64, digest: i64, rev: i64) -> Result<bool, DbErr> {
        reconcile::verify(&self.db, &self.ns, key(rel, row), digest, rev).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sea_orm::{ConnectOptions, Database};

    async fn open() -> RelStore {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("relstore_test_{}_{uniq}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        RelStore::attach(Database::connect(opt).await.unwrap()).await.unwrap()
    }

    // TWO relations in one store: rel 0 (roots/"files"), rel 1 ("derived"). A cross-rel
    // dep 0:0 -> 1:0, and a cycle inside rel 1 (1:0 ->1:1 ->1:2 ->1:0). Cut the root.
    // Cycle-safe retraction must kill the whole rel-1 cycle. Proves it's generic + cyclic.
    #[tokio::test]
    async fn generic_two_relation_cycle() {
        let s = open().await;
        s.add_rows(&[(0, 0, 1), (1, 0, 1), (1, 1, 1), (1, 2, 1)]).await.unwrap();
        s.add_deps(&[
            (0, 0, 1, 0), // file 0:0 is the refCount for derived 1:0
            (1, 0, 1, 1), // cycle in rel 1
            (1, 1, 1, 2),
            (1, 2, 1, 0),
        ])
        .await
        .unwrap();
        assert_eq!(s.alive().await.unwrap(), 4);

        s.retract_dred(&[(0, 0)]).await.unwrap();
        assert_eq!(s.alive().await.unwrap(), 0, "cutting the cross-rel anchor kills the rel-1 cycle");
    }

    // ── GraphStore Epic 1 · stamp goldens ──

    async fn raw_db(tag: &str) -> DatabaseConnection {
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let uniq = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "stamp_{tag}_{}_{uniq}.sqlite",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let mut opt = ConnectOptions::new(format!("sqlite://{}?mode=rwc", path.display()));
        opt.max_connections(1).min_connections(1);
        Database::connect(opt).await.unwrap()
    }

    // (type, name, sql) for every persistent schema object (TEMP tables live in
    // sqlite_temp_master, so this sees only the durable tables + indexes).
    async fn persistent_master(db: &DatabaseConnection) -> Vec<(String, String, String)> {
        use sea_orm::{DatabaseBackend, Statement};
        let rows = db
            .query_all_raw(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT type, name, sql FROM sqlite_master \
                 WHERE name NOT LIKE 'sqlite_%' AND name NOT LIKE '\\_%' ESCAPE '\\' \
                 ORDER BY type, name"
                    .to_owned(),
            ))
            .await
            .unwrap();
        rows.iter()
            .map(|r| {
                (
                    r.try_get_by_index::<String>(0).unwrap_or_default(),
                    r.try_get_by_index::<String>(1).unwrap_or_default(),
                    r.try_get_by_index::<String>(2).unwrap_or_default(),
                )
            })
            .collect()
    }

    // 1.1 — stamp(Split) reproduces the live schemas byte-for-byte. A db stamped
    // via RelStore::attach (Split) has the SAME persistent sqlite_master as one
    // built by cascade::create_schema + reconcile::create_schema directly, and
    // the object set is exactly the six the engine has shipped since the fold.
    #[tokio::test]
    async fn stamp_split_reproduces_the_live_schemas() {
        let via_attach = raw_db("split_attach").await;
        RelStore::attach(via_attach.clone()).await.unwrap();

        let via_direct = raw_db("split_direct").await;
        via_direct
            .execute_unprepared(crate::unfuck_sqlite::OPEN_PRAGMAS)
            .await
            .unwrap();
        crate::cascade::create_schema(&via_direct, &GraphNs::default()).await.unwrap();
        crate::reconcile::create_schema(&via_direct, &GraphNs::default()).await.unwrap();

        let master_attach = persistent_master(&via_attach).await;
        let master_direct = persistent_master(&via_direct).await;
        assert_eq!(
            master_attach, master_direct,
            "stamp(Split) must equal the live create_schema pair, object-for-object"
        );

        let names: Vec<&str> = master_attach.iter().map(|(_, n, _)| n.as_str()).collect();
        for required in [
            "cx_row", "cx_dep", "ix_cx_dep_child", "rx_memo", "rx_dep", "ix_rx_read",
        ] {
            assert!(names.contains(&required), "schema object {required} missing: {names:?}");
        }
    }

    async fn count_alive(db: &DatabaseConnection, table: &str) -> i64 {
        use sea_orm::{DatabaseBackend, Statement};
        db.query_one_raw(Statement::from_string(
            DatabaseBackend::Sqlite,
            format!("SELECT count(*) FROM {table} WHERE weight>0"),
        ))
        .await
        .unwrap()
        .map(|r| r.try_get_by_index::<i64>(0).unwrap_or(0))
        .unwrap_or(0)
    }

    // Epic 3 — two namespaces in ONE db are independent. Stamp the default ns and a
    // "b_" ns into the same db, load the same little ref-count graph into each, retract
    // the anchor in the DEFAULT ns only, and assert the b_ ns's survivors are UNTOUCHED
    // (no cross-talk between cx_row and b_cx_row). Proves the engine is namespace-
    // generic through the cascade, not just at the DDL.
    #[tokio::test]
    async fn two_namespaces_are_independent() {
        let db = raw_db("twons").await;
        // Stamp BOTH namespaces into the one db: cx_*/rx_* and b_cx_*/b_rx_*.
        stamp(&db, &GraphNs::default()).await.unwrap();
        stamp(&db, &GraphNs::new("b_")).await.unwrap();

        // Same ref-count graph in each: (0,0) anchors (0,1) via one dep edge.
        let ns = GraphNs::default();
        let nb = GraphNs::new("b_");
        crate::cascade::insert_rows(&db, &ns, &[(0, 0, 1), (0, 1, 1)]).await.unwrap();
        crate::cascade::insert_deps(&db, &ns, &[(0, 0, 0, 1)]).await.unwrap();
        crate::cascade::insert_rows(&db, &nb, &[(0, 0, 1), (0, 1, 1)]).await.unwrap();
        crate::cascade::insert_deps(&db, &nb, &[(0, 0, 0, 1)]).await.unwrap();
        assert_eq!(count_alive(&db, &ns.row).await, 2);
        assert_eq!(count_alive(&db, &nb.row).await, 2);

        // Retract the anchor in the DEFAULT namespace only.
        crate::cascade::retract(&db, &ns, &[(0, 0)]).await.unwrap();

        // Default ns: anchor + dependent both died. b_ ns: UNTOUCHED (still 2 alive).
        assert_eq!(count_alive(&db, &ns.row).await, 0, "default ns retracted to zero");
        assert_eq!(
            count_alive(&db, &nb.row).await, 2,
            "b_ ns must be untouched by the default ns retract (no cross-talk)"
        );
    }
}

}
