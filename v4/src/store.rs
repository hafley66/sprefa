//! `SprfStore` — content-derived id intern over `Arc<dyn FactStore<Cursor>>`.
//!
//! Layer 0b: ids come from blake3 of content (Layer 0a primitives), not
//! sequential counters. Hot tier is bounded LRU + an exact `HashSet`
//! seen-set per id family. Cold tier is the underlying FactStore;
//! lookups miss-rehydrate through the LRU.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::FactStore;
use lru::LruCache;

use crate::Cursor;
pub use crate::{FileId, PathId, Ref, RepoId, RevId, StringId, WhereBytes, WhereBytesId};

pub const STRINGS_TABLE: &str = "_strings";
pub const FILES_TABLE:   &str = "_files";
pub const WHERE_BYTES_TABLE: &str = "_where_bytes";
pub const REFS_TABLE:    &str = WHERE_BYTES_TABLE;
pub const REPOS_TABLE:   &str = "_repos";
pub const REVS_TABLE:    &str = "_revs";
pub const PATHS_TABLE:   &str = "_paths";
pub const STRING_OBSERVATIONS_TABLE: &str = "_string_observations";

/// Default LRU caps. Single knob per family; can be overridden in tests.
const DEFAULT_STRINGS_CAP: usize = 16_384;
const DEFAULT_FILES_CAP:   usize =  4_096;
const DEFAULT_REFS_CAP:    usize = 16_384;
const DEFAULT_REPOS_CAP:   usize =  1_024;
const DEFAULT_REVS_CAP:    usize =  4_096;
const DEFAULT_PATHS_CAP:   usize = 16_384;
const CORE_FLUSH_ROWS:     usize = 16_384;

fn norm(s: &str) -> String {
    s.chars()
        .filter(|ch| ch.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub struct SprfStore {
    inner:        Arc<dyn FactStore<Cursor>>,
    strings_lru:  Mutex<LruCache<StringId, Arc<str>>>,
    files_lru:    Mutex<LruCache<FileId, ([u8; 32], Arc<str>)>>,
    refs_lru:     Mutex<LruCache<Ref, crate::Coord>>,
    where_bytes_lru: Mutex<LruCache<WhereBytesId, WhereBytes>>,
    repos_lru:    Mutex<LruCache<RepoId, (Arc<str>, Arc<str>)>>,
    revs_lru:     Mutex<LruCache<RevId, (RepoId, Arc<str>, i64)>>,
    paths_lru:    Mutex<LruCache<PathId, (RepoId, RevId, FileId, Arc<str>)>>,
    seen_strings: Mutex<HashSet<u64>>,
    seen_files:   Mutex<HashSet<u64>>,
    seen_refs:    Mutex<HashSet<u64>>,
    seen_repos:   Mutex<HashSet<u64>>,
    seen_revs:    Mutex<HashSet<u64>>,
    seen_paths:   Mutex<HashSet<u64>>,
    seen_string_observations: Mutex<HashSet<u64>>,
    pending_core: Mutex<HashMap<&'static str, Vec<Arc<Cursor>>>>,
}

impl SprfStore {
    pub fn new(inner: Arc<dyn FactStore<Cursor>>) -> Arc<Self> {
        Self::with_caps(
            inner,
            DEFAULT_STRINGS_CAP,
            DEFAULT_FILES_CAP,
            DEFAULT_REFS_CAP,
        )
    }

    pub fn with_caps(
        inner: Arc<dyn FactStore<Cursor>>,
        strings_cap: usize, files_cap: usize, refs_cap: usize,
    ) -> Arc<Self> {
        let nz = |n: usize| NonZeroUsize::new(n.max(1)).unwrap();
        let store = Arc::new(Self {
            inner,
            strings_lru:  Mutex::new(LruCache::new(nz(strings_cap))),
            files_lru:    Mutex::new(LruCache::new(nz(files_cap))),
            refs_lru:     Mutex::new(LruCache::new(nz(refs_cap))),
            where_bytes_lru: Mutex::new(LruCache::new(nz(refs_cap))),
            repos_lru:    Mutex::new(LruCache::new(nz(DEFAULT_REPOS_CAP))),
            revs_lru:     Mutex::new(LruCache::new(nz(DEFAULT_REVS_CAP))),
            paths_lru:    Mutex::new(LruCache::new(nz(DEFAULT_PATHS_CAP))),
            seen_strings: Mutex::new(HashSet::new()),
            seen_files:   Mutex::new(HashSet::new()),
            seen_refs:    Mutex::new(HashSet::new()),
            seen_repos:   Mutex::new(HashSet::new()),
            seen_revs:    Mutex::new(HashSet::new()),
            seen_paths:   Mutex::new(HashSet::new()),
            seen_string_observations: Mutex::new(HashSet::new()),
            pending_core: Mutex::new(HashMap::new()),
        });
        store.declare_core_tables();
        store.preinsert_sentinels();
        store
    }

    /// Underlying FactStore — for ops/tests that read the cold tables.
    pub fn inner(&self) -> &Arc<dyn FactStore<Cursor>> {
        self.flush();
        &self.inner
    }

    pub fn flush(&self) {
        let pending = std::mem::take(&mut *self.pending_core.lock().unwrap());
        for (table, rows) in pending {
            if rows.is_empty() { continue; }
            self.inner.insert_batch(table, rows);
        }
    }

    fn insert_core(&self, table: &'static str, row: Cursor) {
        let should_flush = {
            let mut pending = self.pending_core.lock().unwrap();
            let rows = pending.entry(table).or_default();
            rows.push(Arc::new(row));
            rows.len() >= CORE_FLUSH_ROWS
        };
        if should_flush {
            self.flush();
        }
    }

    fn declare_core_tables(&self) {
        self.inner.declare(STRINGS_TABLE, &["id", "content", "norm"]);
        self.inner.declare(FILES_TABLE, &["id", "content_hash", "path", "size"]);
        self.inner.declare(WHERE_BYTES_TABLE, &["id", "string_id", "file_id", "lo", "hi", "repo", "rev"]);
        self.inner.declare(REPOS_TABLE, &["id", "slug", "remote"]);
        self.inner.declare(REVS_TABLE, &["id", "repo_id", "oid", "ts"]);
        self.inner.declare(PATHS_TABLE, &["id", "repo_id", "rev_id", "file_id", "path"]);
        self.inner.declare(
            STRING_OBSERVATIONS_TABLE,
            &["id", "string_id", "role", "where_bytes_id", "context_kind", "context_id"],
        );
    }

    // ── sentinels ────────────────────────────────────────────────────

    fn preinsert_sentinels(&self) {
        // _strings(0) = ""
        let mut row = Cursor::default();
        row.set("id",        "0");
        row.set("content",   "");
        row.set("norm",      "");
        self.insert_core(STRINGS_TABLE, row);
        self.seen_strings.lock().unwrap().insert(0);
        self.strings_lru.lock().unwrap().put(StringId::EMPTY, Arc::<str>::from(""));

        // _files(0) = synthetic
        let mut row = Cursor::default();
        row.set("id",           "0");
        row.set("content_hash", "0".repeat(64));
        row.set("path",         "\u{2205}");
        row.set("size",         "0");
        self.insert_core(FILES_TABLE, row);
        self.seen_files.lock().unwrap().insert(0);
        let synth_hash = [0u8; 32];
        self.files_lru.lock().unwrap()
            .put(0 as FileId, (synth_hash, Arc::<str>::from("\u{2205}")));

        // _where_bytes(0) = synthetic
        let mut row = Cursor::default();
        row.set("id",      "0");
        row.set("string_id", "0");
        row.set("file_id", "0");
        row.set("lo",      "0");
        row.set("hi",      "0");
        row.set("repo",    "0");
        row.set("rev",     "0");
        self.insert_core(WHERE_BYTES_TABLE, row);
        self.seen_refs.lock().unwrap().insert(0);
        self.refs_lru.lock().unwrap().put(Ref::SYNTHETIC, crate::Coord::default());
        self.where_bytes_lru.lock().unwrap().put(WhereBytesId::SYNTHETIC, WhereBytes::default());

        // _repos(0) = empty slug + remote
        let mut row = Cursor::default();
        row.set("id",     "0");
        row.set("slug",   "");
        row.set("remote", "");
        self.insert_core(REPOS_TABLE, row);
        self.seen_repos.lock().unwrap().insert(0);
        self.repos_lru.lock().unwrap()
            .put(0 as RepoId, (Arc::<str>::from(""), Arc::<str>::from("")));

        // _revs(0) = repo_id=0, empty oid, ts=0
        let mut row = Cursor::default();
        row.set("id",      "0");
        row.set("repo_id", "0");
        row.set("oid",     "");
        row.set("ts",      "0");
        self.insert_core(REVS_TABLE, row);
        self.seen_revs.lock().unwrap().insert(0);
        self.revs_lru.lock().unwrap()
            .put(0 as RevId, (0 as RepoId, Arc::<str>::from(""), 0i64));

        // _paths(0) = synthetic
        let mut row = Cursor::default();
        row.set("id",      "0");
        row.set("repo_id", "0");
        row.set("rev_id",  "0");
        row.set("file_id", "0");
        row.set("path",    "\u{2205}");
        self.insert_core(PATHS_TABLE, row);
        self.seen_paths.lock().unwrap().insert(0);
        self.paths_lru.lock().unwrap().put(
            0 as PathId,
            (0 as RepoId, 0 as RevId, 0 as FileId, Arc::<str>::from("\u{2205}")),
        );

        let mut row = Cursor::default();
        row.set("id",             "0");
        row.set("string_id",      "0");
        row.set("role",           "");
        row.set("where_bytes_id", "0");
        row.set("context_kind",   "");
        row.set("context_id",     "0");
        self.insert_core(STRING_OBSERVATIONS_TABLE, row);
        self.seen_string_observations.lock().unwrap().insert(0);
    }

    // ── strings ──────────────────────────────────────────────────────

    pub fn intern_string(&self, s: &str) -> StringId {
        let id = StringId::of(s);
        if !self.seen_strings.lock().unwrap().insert(id.0) {
            // Already in cold storage (or sentinel). Touch LRU on cache hit.
            let mut lru = self.strings_lru.lock().unwrap();
            if lru.get(&id).is_none() {
                lru.put(id, Arc::<str>::from(s));
            }
            return id;
        }
        let mut row = Cursor::default();
        row.set("id",        id.0.to_string());
        row.set("content",   s);
        row.set("norm",      norm(s));
        self.insert_core(STRINGS_TABLE, row);
        self.strings_lru.lock().unwrap().put(id, Arc::<str>::from(s));
        id
    }

    pub fn lookup_string(&self, id: StringId) -> Option<Arc<str>> {
        if let Some(arc) = self.strings_lru.lock().unwrap().get(&id) {
            return Some(arc.clone());
        }
        let id_str = id.0.to_string();
        self.flush();
        for row in self.inner.rows_of(STRINGS_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let content = row.get("content").unwrap_or("").to_string();
                let arc: Arc<str> = Arc::<str>::from(content);
                self.strings_lru.lock().unwrap().put(id, arc.clone());
                return Some(arc);
            }
        }
        None
    }

    // ── files ────────────────────────────────────────────────────────

    pub fn intern_file(&self, content: &[u8], first_path: &str) -> FileId {
        let id = crate::file_id_of(content);
        let full_hash = blake3::hash(content);
        let hash_bytes = *full_hash.as_bytes();
        if !self.seen_files.lock().unwrap().insert(id) {
            let mut lru = self.files_lru.lock().unwrap();
            if lru.get(&id).is_none() {
                lru.put(id, (hash_bytes, Arc::<str>::from(first_path)));
            }
            return id;
        }
        let mut row = Cursor::default();
        row.set("id",           id.to_string());
        row.set("content_hash", full_hash.to_hex().to_string());
        row.set("path",         first_path);
        row.set("size",         content.len().to_string());
        self.insert_core(FILES_TABLE, row);
        self.files_lru.lock().unwrap()
            .put(id, (hash_bytes, Arc::<str>::from(first_path)));
        id
    }

    pub fn lookup_file(&self, id: FileId) -> Option<([u8; 32], Arc<str>)> {
        if let Some(meta) = self.files_lru.lock().unwrap().get(&id) {
            return Some(meta.clone());
        }
        let id_str = id.to_string();
        self.flush();
        for row in self.inner.rows_of(FILES_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let hash_hex = row.get("content_hash").unwrap_or("").to_string();
                let path     = row.get("path").unwrap_or("").to_string();
                let mut hash = [0u8; 32];
                if hash_hex.len() == 64 {
                    for (i, chunk) in hash_hex.as_bytes().chunks(2).enumerate() {
                        let s = std::str::from_utf8(chunk).ok()?;
                        hash[i] = u8::from_str_radix(s, 16).ok()?;
                    }
                }
                let meta = (hash, Arc::<str>::from(path));
                self.files_lru.lock().unwrap().put(id, meta.clone());
                return Some(meta);
            }
        }
        None
    }

    // ── refs ─────────────────────────────────────────────────────────

    pub fn intern_ref(&self, c: crate::Coord) -> Ref {
        let r = Ref::of(c);
        if !self.seen_refs.lock().unwrap().insert(r.0) {
            let mut lru = self.refs_lru.lock().unwrap();
            if lru.get(&r).is_none() {
                lru.put(r, c);
            }
            let wb = WhereBytes::from(c);
            let mut wb_lru = self.where_bytes_lru.lock().unwrap();
            if wb_lru.get(&WhereBytesId::from(r)).is_none() {
                wb_lru.put(WhereBytesId::from(r), wb);
            }
            return r;
        }
        let mut row = Cursor::default();
        row.set("id",      r.0.to_string());
        row.set("string_id", "0");
        row.set("file_id", c.fs.to_string());
        row.set("lo",      c.lo.to_string());
        row.set("hi",      c.hi.to_string());
        row.set("repo",    c.repo.to_string());
        row.set("rev",     c.rev.to_string());
        self.insert_core(WHERE_BYTES_TABLE, row);
        self.refs_lru.lock().unwrap().put(r, c);
        self.where_bytes_lru.lock().unwrap().put(WhereBytesId::from(r), WhereBytes::from(c));
        r
    }

    pub fn intern_where_bytes(&self, w: WhereBytes) -> WhereBytesId {
        let r = WhereBytesId::of(w);
        if !self.seen_refs.lock().unwrap().insert(r.0) {
            let mut lru = self.refs_lru.lock().unwrap();
            if lru.get(&Ref::from(r)).is_none() {
                lru.put(Ref::from(r), w.into());
            }
            let mut wb_lru = self.where_bytes_lru.lock().unwrap();
            if wb_lru.get(&r).is_none() {
                wb_lru.put(r, w);
            }
            return r;
        }
        let mut row = Cursor::default();
        row.set("id",        r.0.to_string());
        row.set("string_id", w.string.0.to_string());
        row.set("file_id",   w.file.to_string());
        row.set("lo",        w.lo.to_string());
        row.set("hi",        w.hi.to_string());
        row.set("repo",      w.repo.to_string());
        row.set("rev",       w.rev.to_string());
        self.insert_core(WHERE_BYTES_TABLE, row);
        self.refs_lru.lock().unwrap().put(Ref::from(r), w.into());
        self.where_bytes_lru.lock().unwrap().put(r, w);
        r
    }

    pub fn coord_of(&self, r: Ref) -> Option<crate::Coord> {
        if let Some(c) = self.refs_lru.lock().unwrap().get(&r) {
            return Some(*c);
        }
        let id_str = r.0.to_string();
        self.flush();
        for row in self.inner.rows_of(WHERE_BYTES_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let c = crate::Coord {
                    repo: row.get("repo").unwrap_or("0").parse().unwrap_or(0),
                    rev:  row.get("rev").unwrap_or("0").parse().unwrap_or(0),
                    fs:   row.get("file_id").unwrap_or("0").parse().unwrap_or(0),
                    lo:   row.get("lo").unwrap_or("0").parse().unwrap_or(0),
                    hi:   row.get("hi").unwrap_or("0").parse().unwrap_or(0),
                };
                self.refs_lru.lock().unwrap().put(r, c);
                return Some(c);
            }
        }
        None
    }

    pub fn where_bytes_of(&self, r: WhereBytesId) -> Option<WhereBytes> {
        if let Some(w) = self.where_bytes_lru.lock().unwrap().get(&r) {
            return Some(*w);
        }
        let id_str = r.0.to_string();
        self.flush();
        for row in self.inner.rows_of(WHERE_BYTES_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let w = WhereBytes {
                    string: StringId(row.get("string_id").unwrap_or("0").parse().unwrap_or(0)),
                    repo:   row.get("repo").unwrap_or("0").parse().unwrap_or(0),
                    rev:    row.get("rev").unwrap_or("0").parse().unwrap_or(0),
                    file:   row.get("file_id").unwrap_or("0").parse().unwrap_or(0),
                    lo:     row.get("lo").unwrap_or("0").parse().unwrap_or(0),
                    hi:     row.get("hi").unwrap_or("0").parse().unwrap_or(0),
                };
                self.refs_lru.lock().unwrap().put(Ref::from(r), w.into());
                self.where_bytes_lru.lock().unwrap().put(r, w);
                return Some(w);
            }
        }
        None
    }

    pub fn observe_string(
        &self,
        string: StringId,
        role: &str,
        where_bytes: WhereBytesId,
        context_kind: &str,
        context_id: u64,
    ) -> u64 {
        let mut h = blake3::Hasher::new();
        h.update(&string.0.to_be_bytes());
        h.update(b"\0");
        h.update(role.as_bytes());
        h.update(b"\0");
        h.update(&where_bytes.0.to_be_bytes());
        h.update(b"\0");
        h.update(context_kind.as_bytes());
        h.update(b"\0");
        h.update(&context_id.to_be_bytes());
        let bytes = h.finalize();
        let id = u64::from_be_bytes(bytes.as_bytes()[..8].try_into().unwrap());
        if !self.seen_string_observations.lock().unwrap().insert(id) {
            return id;
        }

        let mut row = Cursor::default();
        row.set("id",             id.to_string());
        row.set("string_id",      string.0.to_string());
        row.set("role",           role);
        row.set("where_bytes_id", where_bytes.0.to_string());
        row.set("context_kind",   context_kind);
        row.set("context_id",     context_id.to_string());
        self.insert_core(STRING_OBSERVATIONS_TABLE, row);
        id
    }

    // ── repos ────────────────────────────────────────────────────────
    //
    // Layer 1. Content-derived RepoId = blake3(slug || remote)[..4] as u32.
    // Empty slug + empty remote collapses to the sentinel id 0.

    pub fn intern_repo(&self, slug: &str, remote: &str) -> RepoId {
        if slug.is_empty() && remote.is_empty() { return 0; }
        let mut h = blake3::Hasher::new();
        h.update(slug.as_bytes());
        h.update(b"\0");
        h.update(remote.as_bytes());
        let bytes = h.finalize();
        let id = u32::from_be_bytes(bytes.as_bytes()[..4].try_into().unwrap());
        if !self.seen_repos.lock().unwrap().insert(id as u64) {
            let mut lru = self.repos_lru.lock().unwrap();
            if lru.get(&id).is_none() {
                lru.put(id, (Arc::<str>::from(slug), Arc::<str>::from(remote)));
            }
            return id;
        }
        let mut row = Cursor::default();
        row.set("id",     id.to_string());
        row.set("slug",   slug);
        row.set("remote", remote);
        self.insert_core(REPOS_TABLE, row);
        self.repos_lru.lock().unwrap()
            .put(id, (Arc::<str>::from(slug), Arc::<str>::from(remote)));
        id
    }

    pub fn lookup_repo(&self, id: RepoId) -> Option<(Arc<str>, Arc<str>)> {
        if let Some(meta) = self.repos_lru.lock().unwrap().get(&id) {
            return Some(meta.clone());
        }
        let id_str = id.to_string();
        self.flush();
        for row in self.inner.rows_of(REPOS_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let slug   = row.get("slug").unwrap_or("").to_string();
                let remote = row.get("remote").unwrap_or("").to_string();
                let meta   = (Arc::<str>::from(slug), Arc::<str>::from(remote));
                self.repos_lru.lock().unwrap().put(id, meta.clone());
                return Some(meta);
            }
        }
        None
    }

    // ── revs ─────────────────────────────────────────────────────────
    //
    // RevId = blake3(repo || oid)[..4] as u32. Same oid under different
    // RepoIds yields different RevIds.

    pub fn intern_rev(&self, repo: RepoId, oid: &str, ts: i64) -> RevId {
        if repo == 0 && oid.is_empty() && ts == 0 { return 0; }
        let mut h = blake3::Hasher::new();
        h.update(&repo.to_be_bytes());
        h.update(b"\0");
        h.update(oid.as_bytes());
        let bytes = h.finalize();
        let id = u32::from_be_bytes(bytes.as_bytes()[..4].try_into().unwrap());
        if !self.seen_revs.lock().unwrap().insert(id as u64) {
            let mut lru = self.revs_lru.lock().unwrap();
            if lru.get(&id).is_none() {
                lru.put(id, (repo, Arc::<str>::from(oid), ts));
            }
            return id;
        }
        let mut row = Cursor::default();
        row.set("id",      id.to_string());
        row.set("repo_id", repo.to_string());
        row.set("oid",     oid);
        row.set("ts",      ts.to_string());
        self.insert_core(REVS_TABLE, row);
        self.revs_lru.lock().unwrap()
            .put(id, (repo, Arc::<str>::from(oid), ts));
        id
    }

    pub fn lookup_rev(&self, id: RevId) -> Option<(RepoId, Arc<str>, i64)> {
        if let Some(meta) = self.revs_lru.lock().unwrap().get(&id) {
            return Some(meta.clone());
        }
        let id_str = id.to_string();
        self.flush();
        for row in self.inner.rows_of(REVS_TABLE) {
            if row.get("id").as_deref() == Some(id_str.as_str()) {
                let repo = row.get("repo_id").unwrap_or("0").parse().unwrap_or(0);
                let oid  = row.get("oid").unwrap_or("").to_string();
                let ts   = row.get("ts").unwrap_or("0").parse().unwrap_or(0);
                let meta = (repo, Arc::<str>::from(oid), ts);
                self.revs_lru.lock().unwrap().put(id, meta.clone());
                return Some(meta);
            }
        }
        None
    }

    // ── paths ────────────────────────────────────────────────────────
    //
    // PathId = blake3(repo || rev || file || path)[..8] as u64. Same
    // content under multiple (repo, rev) tuples produces multiple
    // _paths rows but one _files row.

    pub fn intern_path(
        &self, repo: RepoId, rev: RevId, file: FileId, path: &str,
    ) -> PathId {
        if repo == 0 && rev == 0 && file == 0 && path.is_empty() { return 0; }
        let mut h = blake3::Hasher::new();
        h.update(&repo.to_be_bytes());
        h.update(&rev.to_be_bytes());
        h.update(&file.to_be_bytes());
        h.update(path.as_bytes());
        let bytes = h.finalize();
        let id = u64::from_be_bytes(bytes.as_bytes()[..8].try_into().unwrap());
        if !self.seen_paths.lock().unwrap().insert(id) {
            let mut lru = self.paths_lru.lock().unwrap();
            if lru.get(&id).is_none() {
                lru.put(id, (repo, rev, file, Arc::<str>::from(path)));
            }
            return id;
        }
        let mut row = Cursor::default();
        row.set("id",      id.to_string());
        row.set("repo_id", repo.to_string());
        row.set("rev_id",  rev.to_string());
        row.set("file_id", file.to_string());
        row.set("path",    path);
        self.insert_core(PATHS_TABLE, row);
        self.paths_lru.lock().unwrap()
            .put(id, (repo, rev, file, Arc::<str>::from(path)));
        id
    }

    // ── lookups derived from existing tables ────────────────────────

    /// First-seen path of a `_files` row. Thin wrapper over `lookup_file`
    /// — surface for Layer 4 RPCs that don't need the content hash.
    pub fn path_of(&self, f: FileId) -> Option<Arc<str>> {
        self.lookup_file(f).map(|(_, path)| path)
    }

    /// Linear scan over `_paths` for the first row whose `path` column
    /// equals `path`. Returns the FileId. O(n) in `_paths` today; the
    /// SQL index over `(repo_id, rev_id, path)` lands in Layer 4
    /// alongside auto-views.
    pub fn find_file_by_path(&self, path: &str) -> Option<FileId> {
        self.flush();
        for row in self.inner.rows_of(PATHS_TABLE) {
            if row.get("path").as_deref() == Some(path) {
                let file: FileId = row.get("file_id").unwrap_or("0")
                    .parse().unwrap_or(0);
                if file != 0 { return Some(file); }
            }
        }
        None
    }

    /// Linear scan over `_refs` for rows whose coord covers `byte` in
    /// `file`. Returns content-derived `Ref` ids. O(n) in `_refs`
    /// today; SqliteFactStore index over `(file_id, lo)` lands in
    /// Layer 4 alongside auto-views. Acceptable at sprefa scale.
    pub fn find_refs_in(&self, file: FileId, byte: u32) -> Vec<Ref> {
        self.flush();
        let mut out = Vec::new();
        for row in self.inner.rows_of(WHERE_BYTES_TABLE) {
            let f: FileId = row.get("file_id").unwrap_or("0")
                .parse().unwrap_or(0);
            if f != file { continue; }
            let lo: u32 = row.get("lo").unwrap_or("0").parse().unwrap_or(0);
            let hi: u32 = row.get("hi").unwrap_or("0").parse().unwrap_or(0);
            if lo <= byte && byte < hi {
                let id: u64 = row.get("id").unwrap_or("0").parse().unwrap_or(0);
                if id != 0 { out.push(Ref(id)); }
            }
        }
        out
    }

    pub fn find_where_bytes_covering(&self, file: FileId, byte: u32) -> Vec<WhereBytesId> {
        self.find_refs_in(file, byte).into_iter().map(WhereBytesId::from).collect()
    }
}
