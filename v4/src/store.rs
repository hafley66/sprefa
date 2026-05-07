//! `SprfStore` — content-derived id intern over `Arc<dyn FactStore<Cursor>>`.
//!
//! Layer 0b: ids come from blake3 of content (Layer 0a primitives), not
//! sequential counters. Hot tier is bounded LRU + an exact `HashSet`
//! seen-set per id family. Cold tier is the underlying FactStore;
//! lookups miss-rehydrate through the LRU.

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::FactStore;
use lru::LruCache;

use crate::Cursor;
pub use crate::{FileId, Ref, StringId};

pub const STRINGS_TABLE: &str = "_strings";
pub const FILES_TABLE:   &str = "_files";
pub const REFS_TABLE:    &str = "_refs";

/// Default LRU caps. Single knob per family; can be overridden in tests.
const DEFAULT_STRINGS_CAP: usize = 16_384;
const DEFAULT_FILES_CAP:   usize =  4_096;
const DEFAULT_REFS_CAP:    usize = 16_384;

fn norm_ws(s: &str) -> String { s.split_whitespace().collect::<Vec<_>>().join(" ") }
fn norm_case(s: &str) -> String { s.to_lowercase() }

pub struct SprfStore {
    inner:        Arc<dyn FactStore<Cursor>>,
    strings_lru:  Mutex<LruCache<StringId, Arc<str>>>,
    files_lru:    Mutex<LruCache<FileId, ([u8; 32], Arc<str>)>>,
    refs_lru:     Mutex<LruCache<Ref, crate::Coord>>,
    seen_strings: Mutex<HashSet<u64>>,
    seen_files:   Mutex<HashSet<u64>>,
    seen_refs:    Mutex<HashSet<u64>>,
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
            seen_strings: Mutex::new(HashSet::new()),
            seen_files:   Mutex::new(HashSet::new()),
            seen_refs:    Mutex::new(HashSet::new()),
        });
        store.preinsert_sentinels();
        store
    }

    /// Underlying FactStore — for ops/tests that read the cold tables.
    pub fn inner(&self) -> &Arc<dyn FactStore<Cursor>> { &self.inner }

    // ── sentinels ────────────────────────────────────────────────────

    fn preinsert_sentinels(&self) {
        // _strings(0) = ""
        let mut row = Cursor::default();
        row.set("id",        "0");
        row.set("content",   "");
        row.set("norm_ws",   "");
        row.set("norm_case", "");
        self.inner.insert(STRINGS_TABLE, Arc::new(row));
        self.seen_strings.lock().unwrap().insert(0);
        self.strings_lru.lock().unwrap().put(StringId::EMPTY, Arc::<str>::from(""));

        // _files(0) = synthetic
        let mut row = Cursor::default();
        row.set("id",           "0");
        row.set("content_hash", "0".repeat(64));
        row.set("path",         "\u{2205}");
        row.set("size",         "0");
        self.inner.insert(FILES_TABLE, Arc::new(row));
        self.seen_files.lock().unwrap().insert(0);
        let synth_hash = [0u8; 32];
        self.files_lru.lock().unwrap()
            .put(0 as FileId, (synth_hash, Arc::<str>::from("\u{2205}")));

        // _refs(0) = synthetic
        let mut row = Cursor::default();
        row.set("id",      "0");
        row.set("file_id", "0");
        row.set("lo",      "0");
        row.set("hi",      "0");
        row.set("repo",    "0");
        row.set("rev",     "0");
        self.inner.insert(REFS_TABLE, Arc::new(row));
        self.seen_refs.lock().unwrap().insert(0);
        self.refs_lru.lock().unwrap().put(Ref::SYNTHETIC, crate::Coord::default());
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
        row.set("norm_ws",   norm_ws(s));
        row.set("norm_case", norm_case(s));
        self.inner.insert(STRINGS_TABLE, Arc::new(row));
        self.strings_lru.lock().unwrap().put(id, Arc::<str>::from(s));
        id
    }

    pub fn lookup_string(&self, id: StringId) -> Option<Arc<str>> {
        if let Some(arc) = self.strings_lru.lock().unwrap().get(&id) {
            return Some(arc.clone());
        }
        let id_str = id.0.to_string();
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
        self.inner.insert(FILES_TABLE, Arc::new(row));
        self.files_lru.lock().unwrap()
            .put(id, (hash_bytes, Arc::<str>::from(first_path)));
        id
    }

    pub fn lookup_file(&self, id: FileId) -> Option<([u8; 32], Arc<str>)> {
        if let Some(meta) = self.files_lru.lock().unwrap().get(&id) {
            return Some(meta.clone());
        }
        let id_str = id.to_string();
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
            return r;
        }
        let mut row = Cursor::default();
        row.set("id",      r.0.to_string());
        row.set("file_id", c.fs.to_string());
        row.set("lo",      c.lo.to_string());
        row.set("hi",      c.hi.to_string());
        row.set("repo",    c.repo.to_string());
        row.set("rev",     c.rev.to_string());
        self.inner.insert(REFS_TABLE, Arc::new(row));
        self.refs_lru.lock().unwrap().put(r, c);
        r
    }

    pub fn coord_of(&self, r: Ref) -> Option<crate::Coord> {
        if let Some(c) = self.refs_lru.lock().unwrap().get(&r) {
            return Some(*c);
        }
        let id_str = r.0.to_string();
        for row in self.inner.rows_of(REFS_TABLE) {
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
}
