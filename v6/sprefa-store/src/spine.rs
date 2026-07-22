//! The CLOSED v6 data model, in one file: 6 dimensions + unified node/edge.
//!
//! Codd would not lose a limb over this. Third normal form: every non-key
//! attribute depends on the key, the whole key, and nothing but the key. No
//! denormalized twin columns (v5's `_where_bytes` carried repo/rev/path TEXT
//! beside the ids — deleted). Every child column is a real FK `REFERENCES`
//! (owner law). Every id is a dense DB/interner surrogate, never a content hash
//! (v5's `StringId=hash64`, `FileId=hash64` — deleted).
//!
//! The eight tables and their FK edges:
//!
//!   strings(string_id PK)                     <- the interned dictionary
//!   repos(repo_id PK)
//!   repo_revs(rev_id PK, repo_id -> repos)    <- a rev is OWNED by a repo
//!   files(file_id PK, content_hash UNIQUE)    <- content, addressed once
//!   revs_files(rev_id -> repo_revs,           <- one content at many (rev,path)
//!              path_string_id -> strings,        places; path lives ONCE, here
//!              file_id -> files)
//!   file_bytes(file_id -> files,              <- a byte span into content
//!              string_id? -> strings, start, end)
//!   node(node_id PK, file_id -> files,        <- unified graph node, CONTENT-
//!        name_id? -> strings, family, kind)      scoped: no rev column, so a
//!                                                 file identical across revs
//!                                                 stores its nodes ONCE
//!   edge(family, src_id -> node,              <- unified graph edge
//!        dst_id -> node, kind)
//!
//! node/edge are content-scoped on purpose: rev reaches nodes transitively via
//! revs_files -> files -> node. That is what structurally kills v5's `salt_rev`
//! and the four `df_node*` projections.

use sea_orm::sea_query::{Index, SqliteQueryBuilder, TableCreateStatement};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr, Schema};

use crate::unfuck_sqlite::UnfuckSqlite;

/// The unified graph family discriminator. Stored as a small `i32` ordinal on
/// node/edge, never a string (v5 stored `df_node.kind` as a full 8-byte
/// interned string id per row).
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Df = 0,
    Call = 1,
    Type = 2,
    Module = 3,
}

impl Family {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
    pub fn from_i32(v: i32) -> Option<Self> {
        match v {
            0 => Some(Family::Df),
            1 => Some(Family::Call),
            2 => Some(Family::Type),
            3 => Some(Family::Module),
            _ => None,
        }
    }
}

// NOTE: there is no RootKind enum. A checkout being "main" vs a linked worktree
// vs a daemon background copy is operational metadata (how git/the daemon
// manages it), never read by any spine query — the kernel-roots golden test
// proved three same-rev checkouts collapse to one set of rows and nothing
// branches on kind. A root is just (repo, path); WORK is what actually
// distinguishes a checkout's state. (Standing rule: an enum survives only with a
// golden test for it AND its uses. RootKind had none, so it does not exist.)

/// A rev is either a committed point (a git sha, shared across every root that
/// has that commit) or the WORK state of one root (its uncommitted working
/// tree, no sha). v5 called the latter a `worktree_rev`. HEAD is not a kind —
/// HEAD is just "the committed rev a given root currently points at".
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum RevKind {
    Committed = 0,
    Work = 1,
}

impl RevKind {
    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

// ---- dimension: strings -----------------------------------------------------
pub mod strings {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "strings")]
    pub struct Model {
        // Dense id assigned by the resident interner (lasso Spur), NOT the DB.
        // auto_increment = false: we insert the explicit dense id.
        #[sea_orm(primary_key, auto_increment = false)]
        pub string_id: i64,
        #[sea_orm(unique)]
        pub content: String,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            unreachable!("strings has no outgoing FK")
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- dimension: repos -------------------------------------------------------
pub mod repos {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "repos")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub repo_id: i64,
        #[sea_orm(unique)]
        pub slug: String,
        pub root: String,
        pub url: String,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            unreachable!("repos has no outgoing FK")
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- dimension: roots (a repo's checkouts on disk) -------------------------
pub mod roots {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "roots")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub root_id: i64,
        pub repo_id: i64,                 // -> repos
        #[sea_orm(unique)]
        pub path_string_id: i64,          // -> strings (checkout path, once)
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Repo,
        Path,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::Repo => Entity::belongs_to(super::repos::Entity)
                    .from(Column::RepoId)
                    .to(super::repos::Column::RepoId)
                    .into(),
                Relation::Path => Entity::belongs_to(super::strings::Entity)
                    .from(Column::PathStringId)
                    .to(super::strings::Column::StringId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- dimension: repo_revs (committed sha OR a root's WORK tree) -------------
pub mod repo_revs {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "repo_revs")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub rev_id: i64,
        pub repo_id: i64,                 // -> repos (a rev is OWNED by a repo)
        pub kind: i32,                    // RevKind: committed | work
        pub git_sha: Option<Vec<u8>>,     // 20 raw bytes for committed; NULL for WORK
        pub root_id: Option<i64>,         // -> roots, set for WORK (which tree), else NULL
        // The committed rev a WORK diverges from. This is v1's `<sha>+` dirty
        // marker done relationally: instead of mutating the sha string, WORK
        // points at its base HEAD by FK, so "which files have unstaged changes"
        // is a join (WORK file_id != base file_id per path). NULL for committed.
        pub base_rev_id: Option<i64>,     // -> repo_revs (self), set for WORK
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Repo,
        Root,
        Base,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::Repo => Entity::belongs_to(super::repos::Entity)
                    .from(Column::RepoId)
                    .to(super::repos::Column::RepoId)
                    .into(),
                Relation::Root => Entity::belongs_to(super::roots::Entity)
                    .from(Column::RootId)
                    .to(super::roots::Column::RootId)
                    .into(),
                Relation::Base => Entity::belongs_to(Entity)
                    .from(Column::BaseRevId)
                    .to(Column::RevId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- dimension: files (content, addressed once by hash) --------------------
pub mod files {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "files")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub file_id: i64,
        #[sea_orm(unique)]
        pub content_hash: Vec<u8>, // 16 raw bytes (blake3 truncated)
        pub size: i64,
        pub lines: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            unreachable!("files has no outgoing FK")
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- junction: revs_files (content at a (rev, path) place) -----------------
pub mod revs_files {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "revs_files")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub rev_id: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        pub path_string_id: i64, // -> strings; the path lives ONCE, interned
        pub file_id: i64,        // -> files (content)
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Rev,
        Path,
        File,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::Rev => Entity::belongs_to(super::repo_revs::Entity)
                    .from(Column::RevId)
                    .to(super::repo_revs::Column::RevId)
                    .into(),
                Relation::Path => Entity::belongs_to(super::strings::Entity)
                    .from(Column::PathStringId)
                    .to(super::strings::Column::StringId)
                    .into(),
                Relation::File => Entity::belongs_to(super::files::Entity)
                    .from(Column::FileId)
                    .to(super::files::Column::FileId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- coordinate: file_bytes (a span into physical content) -----------------
pub mod file_bytes {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "file_bytes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub file_id: i64, // -> files (content, not a place)
        #[sea_orm(primary_key, auto_increment = false)]
        pub start: i64,
        #[sea_orm(primary_key, auto_increment = false)]
        pub end: i64,
        pub string_id: Option<i64>, // -> strings, OPTIONAL (the span's text)
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        File,
        Str,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::File => Entity::belongs_to(super::files::Entity)
                    .from(Column::FileId)
                    .to(super::files::Column::FileId)
                    .into(),
                Relation::Str => Entity::belongs_to(super::strings::Entity)
                    .from(Column::StringId)
                    .to(super::strings::Column::StringId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- unified graph node (content-scoped, no rev column) --------------------
pub mod node {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "node")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub node_id: i64,
        pub family: i32,
        pub file_id: i64, // -> files
        pub byte_start: i64,
        pub byte_len: i64,
        pub kind: i32,
        pub name_id: Option<i64>, // -> strings, NULL for anonymous nodes
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        File,
        Name,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::File => Entity::belongs_to(super::files::Entity)
                    .from(Column::FileId)
                    .to(super::files::Column::FileId)
                    .into(),
                Relation::Name => Entity::belongs_to(super::strings::Entity)
                    .from(Column::NameId)
                    .to(super::strings::Column::StringId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// ---- unified graph edge -----------------------------------------------------
pub mod edge {
    use sea_orm::entity::prelude::*;
    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[sea_orm(table_name = "edge")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub family: i32,
        #[sea_orm(primary_key, auto_increment = false)]
        pub src_id: i64, // -> node
        #[sea_orm(primary_key, auto_increment = false)]
        pub dst_id: i64, // -> node
        #[sea_orm(primary_key, auto_increment = false)]
        pub kind: i32,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {
        Src,
        Dst,
    }
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            match self {
                Relation::Src => Entity::belongs_to(super::node::Entity)
                    .from(Column::SrcId)
                    .to(super::node::Column::NodeId)
                    .into(),
                Relation::Dst => Entity::belongs_to(super::node::Entity)
                    .from(Column::DstId)
                    .to(super::node::Column::NodeId)
                    .into(),
            }
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

/// The eight table names, sourced from the entities themselves (the single
/// source of truth for names — used by the no-duplication test).
pub fn table_names() -> Vec<String> {
    use sea_orm::Iden;
    vec![
        strings::Entity.to_string(),
        repos::Entity.to_string(),
        roots::Entity.to_string(),
        repo_revs::Entity.to_string(),
        files::Entity.to_string(),
        revs_files::Entity.to_string(),
        file_bytes::Entity.to_string(),
        node::Entity.to_string(),
        edge::Entity.to_string(),
    ]
}

/// Create every spine table. DDL is DERIVED from the entities via
/// `Schema::create_table_from_entity` — there is no hand-written second copy of
/// the columns anywhere, so the model can never drift from the schema. The
/// `bool` is `WITHOUT ROWID` (composite-key junctions).
pub async fn create_all_tables(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    let schema = Schema::new(backend);
    let defs: Vec<(TableCreateStatement, bool)> = vec![
        (schema.create_table_from_entity(strings::Entity), false),
        (schema.create_table_from_entity(repos::Entity), false),
        (schema.create_table_from_entity(roots::Entity), false),
        (schema.create_table_from_entity(repo_revs::Entity), false),
        (schema.create_table_from_entity(files::Entity), false),
        (schema.create_table_from_entity(revs_files::Entity), true),
        (schema.create_table_from_entity(file_bytes::Entity), true),
        (schema.create_table_from_entity(node::Entity), false),
        (schema.create_table_from_entity(edge::Entity), true),
    ];
    for (stmt, without_rowid) in defs {
        let sql = backend.ddl_for(stmt, without_rowid);
        db.execute_unprepared(&sql).await?;
    }
    for sql in secondary_indexes() {
        db.execute_unprepared(&sql).await?;
    }
    Ok(())
}

/// Identity-uniqueness and reverse-traversal indexes not expressible as a
/// single-column `#[sea_orm(unique)]`.
fn secondary_indexes() -> Vec<String> {
    use sea_orm::sea_query::Alias;
    let repo_rev_identity = Index::create()
        .name("ux_repo_revs_identity")
        .table(Alias::new("repo_revs"))
        .col(Alias::new("repo_id"))
        .col(Alias::new("git_sha"))
        .unique()
        .to_string(SqliteQueryBuilder);
    let node_identity = Index::create()
        .name("ux_node_identity")
        .table(Alias::new("node"))
        .col(Alias::new("family"))
        .col(Alias::new("file_id"))
        .col(Alias::new("byte_start"))
        .col(Alias::new("kind"))
        .unique()
        .to_string(SqliteQueryBuilder);
    let revs_files_by_file = Index::create()
        .name("ix_revs_files_by_file")
        .table(Alias::new("revs_files"))
        .col(Alias::new("file_id"))
        .to_string(SqliteQueryBuilder);
    let edge_by_dst = Index::create()
        .name("ix_edge_by_dst")
        .table(Alias::new("edge"))
        .col(Alias::new("family"))
        .col(Alias::new("dst_id"))
        .to_string(SqliteQueryBuilder);
    // Exactly one WORK rev per root. A partial unique index (sea-query has no
    // portable partial-index builder, so this one is raw — it is inherently a
    // SQLite-ism the same way WITHOUT ROWID is).
    let one_work_per_root = "CREATE UNIQUE INDEX IF NOT EXISTS ux_repo_revs_work_root \
                             ON repo_revs (root_id) WHERE kind = 1"
        .to_string();
    vec![
        repo_rev_identity,
        node_identity,
        revs_files_by_file,
        edge_by_dst,
        one_work_per_root,
    ]
}

// ---- folded from rels.rs / transforms.rs / unfuck_sqlite.rs ----
pub mod rels {
//! The OPEN core: mint new rel tables at runtime with the sea-query builder.
//!
//! The nine spine tables are CLOSED — the model. A *rel* is a materialized query
//! result whose table is created on demand; this is where a user program extends
//! storage without touching the model. Every rel table is `WITHOUT ROWID` with a
//! composite PK over its key columns (set semantics, no duplicate autoindex —
//! the all-columns-PK fix). Columns are integers (dense ids into the spine) or
//! text, never a smuggled coordinate.

use sea_orm::sea_query::{Alias, ColumnDef, Index, SqliteQueryBuilder, Table};
use sea_orm::{ConnectionTrait, DatabaseConnection, DbErr};

/// A column in a dynamically-created rel table.
pub enum RelCol {
    /// `NOT NULL` integer — a dense id into the spine, or a small ordinal.
    Int(&'static str),
    /// Nullable integer.
    IntNull(&'static str),
    /// `NOT NULL` text — the rare rel that carries a literal, not an id.
    Text(&'static str),
}

impl RelCol {
    fn name(&self) -> &'static str {
        match self {
            RelCol::Int(n) | RelCol::IntNull(n) | RelCol::Text(n) => n,
        }
    }
}

/// Create rel table `name` with `cols`; the PK is the named `pk` columns and the
/// table is `WITHOUT ROWID`. Idempotent (`IF NOT EXISTS`).
pub async fn create_rel_table(
    db: &DatabaseConnection,
    name: &str,
    cols: &[RelCol],
    pk: &[&str],
) -> Result<(), DbErr> {
    let mut t = Table::create();
    t.table(Alias::new(name)).if_not_exists();
    for c in cols {
        let mut def = ColumnDef::new(Alias::new(c.name()));
        match c {
            RelCol::Int(_) => {
                def.integer().not_null();
            }
            RelCol::IntNull(_) => {
                def.integer();
            }
            RelCol::Text(_) => {
                def.text().not_null();
            }
        }
        t.col(&mut def);
    }
    if !pk.is_empty() {
        let mut idx = Index::create();
        for k in pk {
            idx.col(Alias::new(*k));
        }
        t.primary_key(&mut idx);
    }
    let mut sql = t.to_string(SqliteQueryBuilder);
    if !pk.is_empty() {
        sql.push_str(" WITHOUT ROWID");
    }
    db.execute_unprepared(&sql).await?;
    Ok(())
}

/// Drop a rel table (an evicted / no-longer-subscribed rel leaves zero bytes).
pub async fn drop_rel_table(db: &DatabaseConnection, name: &str) -> Result<(), DbErr> {
    let sql = Table::drop()
        .table(Alias::new(name))
        .if_exists()
        .to_string(SqliteQueryBuilder);
    db.execute_unprepared(&sql).await?;
    Ok(())
}

}
pub mod transforms {
//! Pure pre-storage transforms, done RIGHT. No DB, no state — deterministic
//! functions the Store calls before writing. Each entry names the v5 hack it
//! replaces so the hacks stay dead.

/// Strip to ASCII alphanumeric, lowercase. The ONE v5 transform that was already
/// correct (spine.rs:297). Used for case/punct-insensitive name matching,
/// computed on read — never stored as a duplicate normalized column.
pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// Content hash for a file: blake3 truncated to 16 bytes, the UNIQUE natural key
/// on `files` that dedups byte-identical content to ONE row.
///
/// v5 hack replaced: `FileId = hash64(path+content)` used AS the id (spine.rs:23)
/// — an 8-byte hash that both defeated varint AND could not dedup identical
/// bytes across two paths. Here the surrogate is the dense DB rowid and the hash
/// is only the natural key.
pub fn content_hash(bytes: &[u8]) -> [u8; 16] {
    let h = blake3::hash(bytes);
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.as_bytes()[..16]);
    out
}

/// A 40-char git sha hex string -> 20 raw bytes for `repo_revs.git_sha`.
///
/// v5 hack replaced: the 40-char hex was stored as TEXT and, worse, smuggled
/// into interned id strings via `salt_rev` (`format!("{rev}\u{1}{id}")`). Here
/// it is 20 raw bytes in exactly one column and nowhere else.
pub fn git_sha_bytes(hex: &str) -> Option<[u8; 20]> {
    if hex.len() != 40 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 20];
    for (i, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Byte offset -> (line, col), 0-based, derived from content on demand.
///
/// v5 hack replaced: `file:line:col` was baked INTO dictionary strings (10.2% of
/// the dict) and even hashed into node ids (`StringId::of(format!("{file}:{line}:
/// {col}:{kind}"))`). Here a node is `(file_id, byte_start)` and line/col are
/// DERIVED here when a human needs to read them — never stored.
pub fn byte_to_linecol(content: &[u8], offset: usize) -> (u32, u32) {
    let end = offset.min(content.len());
    let mut line = 0u32;
    let mut col = 0u32;
    for &b in &content[..end] {
        if b == b'\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

}
pub mod unfuck_sqlite {
//! Every SQLite-specific unfuckening, behind a trait so nothing else in the
//! crate touches a raw pragma or a dialect quirk. "A trait over whatever it
//! needs to be a trait" — implemented on `DatabaseBackend`.

use sea_orm::sea_query::{SqliteQueryBuilder, TableCreateStatement};
use sea_orm::DatabaseBackend;

/// Pragmas a fresh store opens with:
///  - `foreign_keys=ON`  — FKs are enforced, not decorative (owner law: wrong
///                          FK usage = unplugged).
///  - `journal_mode=WAL` — concurrent readers while the writer ticks.
///  - `synchronous=NORMAL`, `temp_store=MEMORY` — the ast-grep/biome-speed knobs.
pub const OPEN_PRAGMAS: &str = "PRAGMA journal_mode=WAL;\
PRAGMA synchronous=NORMAL;\
PRAGMA foreign_keys=ON;\
PRAGMA temp_store=MEMORY;";

pub trait UnfuckSqlite {
    /// DDL for a table-create statement, unfucked for SQLite:
    ///
    ///  1. **AUTOINCREMENT stripped.** A plain `INTEGER PRIMARY KEY` is the
    ///     rowid alias — dense, free, no extra B-tree. `AUTOINCREMENT` only adds
    ///     the `sqlite_sequence` bookkeeping table to prevent rowid reuse, which
    ///     we never need. (v5 never hit this because v5 never used a DB-assigned
    ///     id — it hashed content into the id. This is the fix for doing it right.)
    ///  2. **`WITHOUT ROWID` appended** for composite-key junctions. The table
    ///     becomes its own PK index instead of paying for a rowid heap PLUS a
    ///     full-width duplicate autoindex — the 256.6 MB duplicate-copy bucket
    ///     that all-columns PKs cost v5.
    fn ddl_for(&self, stmt: TableCreateStatement, without_rowid: bool) -> String;
}

impl UnfuckSqlite for DatabaseBackend {
    fn ddl_for(&self, stmt: TableCreateStatement, without_rowid: bool) -> String {
        let mut sql = stmt.to_string(SqliteQueryBuilder);
        sql = sql.replace(" AUTOINCREMENT", "");
        if without_rowid {
            sql.push_str(" WITHOUT ROWID");
        }
        sql
    }
}

}
