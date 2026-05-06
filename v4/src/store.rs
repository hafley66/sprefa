//! `SprfStore` — sprefa-side wrapper over `Arc<dyn FactStore<Cursor>>`.
//!
//! Layer call: effect_runtime owns the generic FactStore (Mem/Sqlite,
//! `<R: Row>`); SprfStore lives here because the strings/refs/files
//! convention is sprefa-domain. Ops never touch FactStore directly —
//! they go through SprfStore so capture stamping, span FK rows, and
//! content-hash file identity are all centralized.
//!
//! Today this ships:
//!   • `_strings(id, content)`           — interned text bag
//!   • `_files(id, content_hash, path)`  — content-addressable file id;
//!     a file unchanged across revs is one row.
//!   • `_refs(id, file_id, lo, hi)`      — span addressable by ref_id
//!
//! Each meta-table is just a normal FactStore table. SprfStore keeps
//! in-memory dedup maps so intern is O(1) hot-path; rows go to the
//! underlying FactStore for persistence + downstream JOINs.
//!
//! Op stamping migration is staged: this commit lands the store +
//! intern API only. `re`/`glob`/`ast`/`comment` keep their stringly
//! `LO`/`HI`/`FS` columns until a follow-up rewires them via
//! `bind_capture`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use effect_runtime::v2::FactStore;

use crate::Cursor;

pub type StringId = u64;
pub type FileId   = u64;
pub type RefId    = u64;

pub const STRINGS_TABLE: &str = "_strings";
pub const FILES_TABLE:   &str = "_files";
pub const REFS_TABLE:    &str = "_refs";

#[derive(Default)]
struct StringIntern {
    fwd: RwLock<HashMap<Arc<str>, StringId>>,
    rev: RwLock<Vec<Arc<str>>>,
}

#[derive(Default)]
struct FileIntern {
    /// `blake3(content)` truncated → FileId. u64 keyed for cheap maps;
    /// the full 32-byte hash also lives on the `_files` row for any
    /// caller that wants the exact digest.
    fwd: RwLock<HashMap<u64, FileId>>,
    /// id → (full blake3 bytes, first-seen path)
    rev: RwLock<Vec<([u8; 32], Arc<str>)>>,
}

#[derive(Default)]
struct RefIntern {
    /// `(file_id, lo, hi) → RefId` dedup. Same span emitted twice
    /// returns the same ref_id (matches v0 ref-uniqueness).
    fwd: RwLock<HashMap<(FileId, u32, u32), RefId>>,
    rev: RwLock<Vec<(FileId, u32, u32)>>,
}

pub struct SprfStore {
    inner:   Arc<dyn FactStore<Cursor>>,
    strings: StringIntern,
    files:   FileIntern,
    refs:    RefIntern,
}

impl SprfStore {
    pub fn new(inner: Arc<dyn FactStore<Cursor>>) -> Arc<Self> {
        Arc::new(Self {
            inner,
            strings: StringIntern::default(),
            files:   FileIntern::default(),
            refs:    RefIntern::default(),
        })
    }

    /// Underlying FactStore — for ops that already stamp wide rules.
    /// New stamping sites should prefer `intern_*` + `bind_capture`.
    pub fn inner(&self) -> &Arc<dyn FactStore<Cursor>> { &self.inner }

    // ── strings ──────────────────────────────────────────────────────

    pub fn intern_string(&self, s: &str) -> StringId {
        if let Some(id) = self.strings.fwd.read().unwrap().get(s).copied() {
            return id;
        }
        let key: Arc<str> = Arc::<str>::from(s);
        let mut fwd = self.strings.fwd.write().unwrap();
        if let Some(id) = fwd.get(&key).copied() { return id; }
        let mut rev = self.strings.rev.write().unwrap();
        let id = rev.len() as StringId;
        rev.push(key.clone());
        fwd.insert(key.clone(), id);

        let mut row = Cursor::default();
        row.set("id",      id.to_string());
        row.set("content", s);
        self.inner.insert(STRINGS_TABLE, Arc::new(row));
        id
    }

    pub fn lookup_string(&self, id: StringId) -> Option<Arc<str>> {
        self.strings.rev.read().unwrap().get(id as usize).cloned()
    }

    // ── files ────────────────────────────────────────────────────────

    /// Content-addressable file identity. Two paths with identical
    /// bytes → same FileId. `first_path` lands on the `_files` row the
    /// first time this content is seen; subsequent calls keep the
    /// original path even if seen via a different on-disk location.
    pub fn intern_file(&self, content: &[u8], first_path: &str) -> FileId {
        let h = blake3::hash(content);
        let key = u64::from_be_bytes(h.as_bytes()[..8].try_into().unwrap());
        if let Some(id) = self.files.fwd.read().unwrap().get(&key).copied() {
            return id;
        }
        let mut fwd = self.files.fwd.write().unwrap();
        if let Some(id) = fwd.get(&key).copied() { return id; }
        let mut rev = self.files.rev.write().unwrap();
        let id = rev.len() as FileId;
        rev.push((*h.as_bytes(), Arc::<str>::from(first_path)));
        fwd.insert(key, id);

        let mut row = Cursor::default();
        row.set("id",            id.to_string());
        row.set("content_hash",  h.to_hex().to_string());
        row.set("path",          first_path);
        self.inner.insert(FILES_TABLE, Arc::new(row));
        id
    }

    pub fn lookup_file(&self, id: FileId) -> Option<([u8; 32], Arc<str>)> {
        self.files.rev.read().unwrap().get(id as usize).cloned()
    }

    // ── refs ─────────────────────────────────────────────────────────

    pub fn intern_ref(&self, file: FileId, lo: u32, hi: u32) -> RefId {
        let key = (file, lo, hi);
        if let Some(id) = self.refs.fwd.read().unwrap().get(&key).copied() {
            return id;
        }
        let mut fwd = self.refs.fwd.write().unwrap();
        if let Some(id) = fwd.get(&key).copied() { return id; }
        let mut rev = self.refs.rev.write().unwrap();
        let id = rev.len() as RefId;
        rev.push(key);
        fwd.insert(key, id);

        let mut row = Cursor::default();
        row.set("id",      id.to_string());
        row.set("file_id", file.to_string());
        row.set("lo",      lo.to_string());
        row.set("hi",      hi.to_string());
        self.inner.insert(REFS_TABLE, Arc::new(row));
        id
    }

    pub fn lookup_ref(&self, id: RefId) -> Option<(FileId, u32, u32)> {
        self.refs.rev.read().unwrap().get(id as usize).copied()
    }

    // ── capture stamping helper ─────────────────────────────────────
    //
    // Op port path: stamp legacy text columns AND new `_REF`/`_STRING`
    // FK columns. Once every emitter is on `bind_capture`, drop the
    // legacy text branch.

    pub fn bind_capture(
        &self, cur: &mut Cursor,
        name: &str, file: FileId, lo: u32, hi: u32, slice: &str,
    ) {
        let s = self.intern_string(slice);
        let r = self.intern_ref(file, lo, hi);
        cur.set(name, slice);
        cur.set(&format!("{name}_REF"),    r.to_string());
        cur.set(&format!("{name}_STRING"), s.to_string());
    }
}
