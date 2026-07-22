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

pub mod benchgraph;
pub mod cascade;
pub mod measure;
pub mod memcap;
pub mod reach;
pub mod reconcile;
pub mod relstore;
pub mod rels;
pub mod spine;
pub mod strings;
pub mod temporal;
pub mod transforms;
pub mod unfuck_sqlite;

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
