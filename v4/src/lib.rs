pub mod app;
pub mod chan;
pub mod compile;
pub mod config;
pub mod cst;
pub mod cursor_codec;
pub mod fact;
#[cfg(feature = "ghcache")]
pub mod ghcache;
#[cfg(feature = "ghcache")]
pub mod git_watch;
pub mod lsp;
pub mod mounted_query;
pub mod pipeline;
pub mod rule;
pub mod runtime_bridge;
pub mod source;
pub mod sprf_introspect;
pub mod sql;
pub mod store;
pub mod telemetry;
pub mod template;
pub mod term;
pub mod v2_ops;

pub use compile::lower;
pub use config::{RepoConfig, SprfConfig};

// sprefa v4 — runtime lib. shared by v4-proto (demo) and v4-bench (perf).
//
//   Layers:
//     §1 Action / Gen
//     §2 Cursor (+ Row impl, Interner)
//     §4 Telemetry / Span
//     §5 Action / lineage counters
//
//   Fact storage is `effect_runtime::v2::FactStore<Cursor>`; impls
//   are `MemFactStore` / `SqliteFactStore`. Re-exported via
//   `crate::fact`.

use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;

use crate::store::SprfStore;

// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░
// ░░░▒                  § 1   actions / dispatch                    ▒░░░
// ░░░▒▒▒▓▓▓██████████████████████████████████████████████████████▓▓▓▒▒▒░░░

pub type Gen = u64;
pub type LineageId = u64;

#[derive(Clone, Debug)]
pub struct Action {
    pub gen: Gen,
    pub parent: Option<(Gen, LineageId)>,
    pub kind: ActionKind,
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    Run { root: PathBuf },
    FileChanged { path: PathBuf },
    Quit,
}

// ╔═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╗
// ║   § 2a  coordinate primitives — Coord, Ref, StringId, Term    ║
// ╚═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╝
//
// Foundation for the coordinate-space cursor. See
// ~/.claude/plans/glittery-napping-whisper.md.
//
// Ids are CONTENT-DERIVED via blake3 truncated to u64. Same content =>
// same id across runs, machines, processes. This makes the strings/refs
// stores lazy-stable: no central allocator, no fwd-map needed, restart
// preserves ids, sqlite cold tier is the source of truth, in-memory
// LRU is the hot cache.
//
// SYNTHETIC sentinels: Coord::default() / Ref(0) / StringId(0) / etc.
// `_strings(0)` is the empty string, pre-interned. `_refs(0)` covers
// no real bytes. Synthetic terms (str/sh/internal) carry SYNTHETIC at
// the coord position but a real StringId at the value position so
// `_strings` dedupes synthetic and source-located text uniformly.

pub type RepoId = u32;
pub type RevId = u32;
pub type FileId = u64;
pub type RefId = u64;
pub type PathId = u64;
pub type BlobId = u64;

/// A coordinate in the (repo × rev × file × byte-range) space.
/// All zeros = SYNTHETIC. Source-located = nonzero ids + real bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Coord {
    pub repo: RepoId,
    pub rev: RevId,
    pub fs: FileId,
    pub lo: u32,
    pub hi: u32,
}

/// Physical byte span in a repo/rev/file source.
///
/// This is the clearer successor name for `Coord`. The old `Coord`
/// remains during migration because many operators still use the `fs`
/// field name directly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct WhereBytes {
    pub string: StringId,
    pub repo: RepoId,
    pub rev: RevId,
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

impl From<Coord> for WhereBytes {
    fn from(c: Coord) -> Self {
        Self {
            string: StringId::EMPTY,
            repo: c.repo,
            rev: c.rev,
            file: c.fs,
            lo: c.lo,
            hi: c.hi,
        }
    }
}

impl From<WhereBytes> for Coord {
    fn from(w: WhereBytes) -> Self {
        Self {
            repo: w.repo,
            rev: w.rev,
            fs: w.file,
            lo: w.lo,
            hi: w.hi,
        }
    }
}

/// FK handle into `_refs`. `Ref(0)` is the SYNTHETIC sentinel. Coord
/// is the value, Ref is the identity. Derive via `Ref::of(coord)` so
/// the id is content-stable across processes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Ref(pub RefId);

impl Ref {
    /// Synthetic Ref. Resolves to `_refs(0)` which covers no bytes.
    pub const SYNTHETIC: Ref = Ref(0);

    /// Content-derived id from a Coord. blake3 truncated to u64. Stable
    /// across runs/machines because the inputs (repo/rev/file/lo/hi)
    /// are themselves content-derived ids.
    pub fn of(c: Coord) -> Ref {
        if c == Coord::default() {
            return Ref::SYNTHETIC;
        }
        let mut h = blake3::Hasher::new();
        h.update(&c.repo.to_be_bytes());
        h.update(&c.rev.to_be_bytes());
        h.update(&c.fs.to_be_bytes());
        h.update(&c.lo.to_be_bytes());
        h.update(&c.hi.to_be_bytes());
        let bytes = h.finalize();
        Ref(u64::from_be_bytes(
            bytes.as_bytes()[..8].try_into().unwrap(),
        ))
    }
}

/// Id for a persisted `WhereBytes` row.
///
/// This is the clearer successor name for `Ref`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct WhereBytesId(pub RefId);

impl WhereBytesId {
    pub const SYNTHETIC: WhereBytesId = WhereBytesId(0);

    pub fn of(w: WhereBytes) -> WhereBytesId {
        if w == WhereBytes::default() {
            return WhereBytesId::SYNTHETIC;
        }
        if w.string == StringId::EMPTY {
            let r = Ref::of(w.into());
            return WhereBytesId(r.0);
        }
        let mut h = blake3::Hasher::new();
        h.update(&w.string.0.to_be_bytes());
        h.update(&w.repo.to_be_bytes());
        h.update(&w.rev.to_be_bytes());
        h.update(&w.file.to_be_bytes());
        h.update(&w.lo.to_be_bytes());
        h.update(&w.hi.to_be_bytes());
        let bytes = h.finalize();
        WhereBytesId(u64::from_be_bytes(
            bytes.as_bytes()[..8].try_into().unwrap(),
        ))
    }

    pub fn coord_only(w: WhereBytes) -> WhereBytesId {
        let r = Ref::of(w.into());
        WhereBytesId(r.0)
    }
}

impl From<Ref> for WhereBytesId {
    fn from(r: Ref) -> Self {
        WhereBytesId(r.0)
    }
}

impl From<WhereBytesId> for Ref {
    fn from(r: WhereBytesId) -> Self {
        Ref(r.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CursorValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(u64),
    String(StringId),
    WhereBytes(WhereBytesId),
    Blob(BlobId),
}

impl Default for CursorValue {
    fn default() -> Self {
        CursorValue::String(StringId::EMPTY)
    }
}

/// FK handle into `_strings`. `StringId(0)` is the empty string,
/// pre-interned at startup so Default round-trips without a row write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct StringId(pub u64);

impl StringId {
    /// The empty string. Pre-interned at id 0.
    pub const EMPTY: StringId = StringId(0);

    /// Content-derived id. `blake3(text)[..8]`. Same text => same id
    /// everywhere, every run, every process.
    pub fn of(text: &str) -> StringId {
        if text.is_empty() {
            return StringId::EMPTY;
        }
        let h = blake3::hash(text.as_bytes());
        StringId(u64::from_be_bytes(h.as_bytes()[..8].try_into().unwrap()))
    }
}

/// Content-derived FileId from raw bytes.
pub fn file_id_of(content: &[u8]) -> FileId {
    if content.is_empty() {
        return 0;
    }
    let h = blake3::hash(content);
    u64::from_be_bytes(h.as_bytes()[..8].try_into().unwrap())
}

pub const FOCAL_TERM: &str = "&";

/// A cursor term. The focal cursor is the term named `&`; `Cursor`
/// derefs to that term so `cursor.value` / `cursor.at` field access
/// delegates to `&`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CursorTerm {
    pub name: Arc<str>,
    pub value: Arc<str>,
    pub cursor_value: CursorValue,
    pub value_id: StringId,
    pub at: Ref,
}

pub type Term = CursorTerm;

impl CursorTerm {
    pub fn new(name: impl Into<Arc<str>>, value: impl Into<Arc<str>>) -> Self {
        let name = name.into();
        let value = value.into();
        let value_id = StringId::of(value.as_ref());
        Self {
            name,
            value,
            cursor_value: CursorValue::String(value_id),
            value_id,
            at: Ref::SYNTHETIC,
        }
    }

    pub fn focal() -> Self {
        Self::new(FOCAL_TERM, "")
    }

    pub fn set_value_arc(&mut self, value: Arc<str>) {
        self.value = value;
        self.value_id = StringId::of(self.value.as_ref());
        self.cursor_value = CursorValue::String(self.value_id);
        self.at = Ref::SYNTHETIC;
    }
}

#[cfg(test)]
mod coord_tests {
    use super::*;

    #[test]
    fn synthetic_sentinels_are_zero() {
        assert_eq!(Ref::SYNTHETIC.0, 0);
        assert_eq!(StringId::EMPTY.0, 0);
        assert_eq!(file_id_of(b""), 0);
        assert_eq!(
            Coord::default(),
            Coord {
                repo: 0,
                rev: 0,
                fs: 0,
                lo: 0,
                hi: 0
            }
        );
        assert_eq!(Ref::of(Coord::default()), Ref::SYNTHETIC);
    }

    #[test]
    fn ids_are_content_derived_and_stable() {
        // Same content => same id every time.
        let a = StringId::of("alpha");
        let b = StringId::of("alpha");
        let c = StringId::of("beta");
        assert_eq!(a, b);
        assert_ne!(a, c);

        // Empty content uses the pre-interned sentinel without hashing.
        assert_eq!(StringId::of(""), StringId::EMPTY);

        // Coord -> Ref likewise stable.
        let coord = Coord {
            repo: 1,
            rev: 2,
            fs: 3,
            lo: 100,
            hi: 200,
        };
        assert_eq!(Ref::of(coord), Ref::of(coord));

        // Different coords -> different refs.
        let other = Coord {
            repo: 1,
            rev: 2,
            fs: 3,
            lo: 101,
            hi: 200,
        };
        assert_ne!(Ref::of(coord), Ref::of(other));
    }

    #[test]
    fn term_synthetic_keeps_real_string() {
        let t = Term::new(":fan_idx", "0");
        assert_eq!(t.at, Ref::SYNTHETIC);
        assert_eq!(t.name.as_ref(), ":fan_idx");
        assert_eq!(t.value.as_ref(), "0");
        assert_eq!(t.value_id, StringId::of("0"));
        // Same value text from a different synthetic term collapses to
        // the same StringId (the dedup property the plan promises).
        let t2 = Term::new(":counter", "0");
        assert_eq!(t.value_id, t2.value_id);
    }
}

// ╔═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╦═══╗
// ║         § 2   cursor — dynamic-scope term-capture bag         ║
// ╚═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╩═══╝
//
// LAYER 0c.2: focal term shape.
//
// The cursor's implicit value is the term named `&`. Cursor derefs to
// that term so existing `cursor.value`, `cursor.at`, `cursor.value_id`,
// and `cursor.cursor_value` field access still names the focal term
// during the CursorValue migration.
#[derive(Clone, Debug)]
pub struct Cursor {
    pub terms: Vec<Term>,
    /// Process-local handle into the intern store. Codec-skipped (Weak
    /// is not portable). Constructors that have an `Arc<SprfStore>`
    /// can opt in via `with_store`; default is None.
    store: Option<Weak<SprfStore>>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            terms: vec![Term::focal()],
            store: None,
        }
    }
}

impl PartialEq for Cursor {
    fn eq(&self, other: &Self) -> bool {
        self.terms == other.terms
    }
}

impl Eq for Cursor {}

impl std::hash::Hash for Cursor {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.terms.hash(state);
    }
}

impl PartialOrd for Cursor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Cursor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.terms.cmp(&other.terms)
    }
}

impl Deref for Cursor {
    type Target = CursorTerm;

    fn deref(&self) -> &Self::Target {
        self.focal()
    }
}

impl DerefMut for Cursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.focal_mut()
    }
}

impl Cursor {
    fn focal_index(&self) -> Option<usize> {
        self.terms
            .iter()
            .position(|t| t.name.as_ref() == FOCAL_TERM)
    }

    fn ensure_focal_index(&mut self) -> usize {
        if let Some(i) = self.focal_index() {
            return i;
        }
        self.terms.insert(0, Term::focal());
        0
    }

    pub fn focal(&self) -> &CursorTerm {
        self.focal_index()
            .and_then(|i| self.terms.get(i))
            .unwrap_or_else(|| panic!("cursor missing focal term `{FOCAL_TERM}`"))
    }

    pub fn focal_mut(&mut self) -> &mut CursorTerm {
        let i = self.ensure_focal_index();
        &mut self.terms[i]
    }

    pub fn set_value(&mut self, value: impl Into<Arc<str>>) {
        self.set_value_arc(value.into());
    }

    pub fn set_value_arc(&mut self, value: Arc<str>) {
        self.focal_mut().set_value_arc(value);
    }

    pub fn set_focal_at(&mut self, value: impl Into<Arc<str>>, at: Ref, cursor_value: CursorValue) {
        let focal = self.focal_mut();
        focal.value = value.into();
        focal.value_id = StringId::of(focal.value.as_ref());
        focal.cursor_value = cursor_value;
        focal.at = at;
    }

    pub fn set(&mut self, name: &str, value: impl Into<Arc<str>>) {
        self.set_arc(name, value.into());
    }

    /// Set with a pre-built Arc<str> value. Use this with Interner so
    /// repeated values (e.g. file paths) share heap.
    pub fn set_arc(&mut self, name: &str, value: Arc<str>) {
        if name == FOCAL_TERM || name == "&.value" {
            self.set_value_arc(value);
            return;
        }
        if let Some(term) = self.terms.iter_mut().find(|t| t.name.as_ref() == name) {
            term.set_value_arc(value);
        } else {
            self.terms.push(Term::new(name, value));
        }
    }

    pub fn set_term_from(&mut self, name: &str, source: &Term) {
        if name == FOCAL_TERM || name == "&.value" {
            self.set_focal_at(source.value.clone(), source.at, source.cursor_value);
            return;
        }
        self.terms.retain(|t| t.name.as_ref() != name);
        self.remove_term_projections(name);
        let mut term = source.clone();
        term.name = Arc::<str>::from(name);
        self.terms.push(term);
        self.project_term_coord(name, source.at);
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        if name == FOCAL_TERM || name == "&.value" {
            return Some(self.focal().value.as_ref());
        }
        if let Some(term) = self.terms.iter().find(|t| t.name.as_ref() == name) {
            return Some(term.value.as_ref());
        }
        // Dotted access compatibility while coord fields are still being
        // stamped as normal terms by older operators.
        if let Some((stem, field)) = name.split_once('.') {
            if stem == "&" {
                return match field {
                    "value" => Some(self.focal().value.as_ref()),
                    other => {
                        let key = other.to_ascii_uppercase();
                        self.get(key.as_str())
                    }
                };
            }
            if field == "value" {
                return self.get(stem);
            }
            let key = format!("{}_{}", stem, field.to_ascii_uppercase());
            return self.get(key.as_str());
        }
        None
    }

    pub fn unset(&mut self, name: &str) {
        if name == FOCAL_TERM || name == "&.value" {
            self.set_value_arc(Arc::<str>::from(""));
            return;
        }
        self.terms.retain(|t| t.name.as_ref() != name);
    }

    /// `&.value`. The focal value of the current cursor.
    pub fn value(&self) -> &str {
        self.focal().value.as_ref()
    }

    // ── Layer 0c coord-space accessors (no live callers in 0c.1; live
    // ── in 0c.2 when emitters migrate to set_at). Allowed-dead so the
    // ── lib.rs build stays clean while these remain unused.

    /// Insert/replace a coord-space term. Interns name + slice through
    /// the store, derives `at` from the child coord. Idempotent on name.
    #[allow(dead_code)]
    pub fn set_at(
        &mut self,
        name: &str,
        slice: &str,
        child_coord: Coord,
        store: &SprfStore,
    ) -> Ref {
        let value_id = store.intern_string(slice);
        let at = Ref::from(store.intern_where_bytes(WhereBytes {
            string: value_id,
            ..WhereBytes::from(child_coord)
        }));
        self.terms.retain(|t| t.name.as_ref() != name);
        self.remove_term_projections(name);
        self.terms.push(Term {
            name: Arc::<str>::from(name),
            value: Arc::<str>::from(slice),
            cursor_value: CursorValue::String(value_id),
            value_id,
            at,
        });
        self.project_term_coord(name, at);
        at
    }

    /// Insert/replace a synthetic coord-space term. Interns name+text
    /// through the store; the `at` slot stays SYNTHETIC.
    #[allow(dead_code)]
    pub fn set_synthetic(&mut self, name: &str, text: &str, store: &SprfStore) {
        let value_id = store.intern_string(text);
        self.terms.retain(|t| t.name.as_ref() != name);
        self.remove_term_projections(name);
        self.terms.push(Term {
            name: Arc::<str>::from(name),
            value: Arc::<str>::from(text),
            cursor_value: CursorValue::String(value_id),
            value_id,
            at: Ref::SYNTHETIC,
        });
    }

    fn remove_term_projections(&mut self, name: &str) {
        let prefixes = [
            format!("{name}_LO"),
            format!("{name}_HI"),
            format!("{name}_FS"),
            format!("{name}_REPO"),
            format!("{name}_REV"),
        ];
        self.terms
            .retain(|t| !prefixes.iter().any(|p| t.name.as_ref() == p));
    }

    fn project_term_coord(&mut self, name: &str, at: Ref) {
        if at == Ref::SYNTHETIC {
            return;
        }
        let Some(store) = self.store.as_ref().and_then(|s| s.upgrade()) else {
            return;
        };
        let Some(coord) = store.coord_of(at) else {
            return;
        };
        self.set(&format!("{name}_LO"), coord.lo.to_string());
        self.set(&format!("{name}_HI"), coord.hi.to_string());
        self.set(&format!("{name}_REPO"), coord.repo.to_string());
        self.set(&format!("{name}_REV"), coord.rev.to_string());
        if let Some(path) = store.path_of(coord.fs) {
            self.set(&format!("{name}_FS"), path);
        } else {
            self.set(&format!("{name}_FS"), coord.fs.to_string());
        }
    }

    /// Look up a coord-space term by interned name id.
    #[allow(dead_code)]
    pub fn term(&self, name_id: StringId) -> Option<&Term> {
        self.terms
            .iter()
            .find(|t| StringId::of(t.name.as_ref()) == name_id)
    }

    /// Builder: attach a process-local Weak handle to the intern store.
    /// Round-trips through the codec as None (Weak is not portable);
    /// callers that need the store re-attach explicitly.
    #[allow(dead_code)]
    pub fn with_store(mut self, store: &Arc<SprfStore>) -> Self {
        self.store = Some(Arc::downgrade(store));
        self
    }
}

impl effect_runtime::v2::Row for Cursor {
    fn get(&self, col: &str) -> Option<&str> {
        Cursor::get(self, col)
    }
    fn set(&mut self, col: &str, value: &str) {
        Cursor::set(self, col, value);
    }
    fn fields(&self) -> Vec<(&str, &str)> {
        self.terms
            .iter()
            .map(|t| (t.name.as_ref(), t.value.as_ref()))
            .collect()
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::*;

    #[test]
    fn focal_ampersand_is_the_public_value_term() {
        let mut c = Cursor::default();
        c.set("&", "hello");

        assert_eq!(c.value(), "hello");
        assert_eq!(c.get("&"), Some("hello"));
        assert_eq!(c.get("&.value"), Some("hello"));
        assert_eq!(c.value_id, StringId::of("hello"));
        assert_eq!(c.cursor_value, CursorValue::String(StringId::of("hello")));
        assert_eq!(c.at, Ref::SYNTHETIC);

        c.set("&.value", "world");
        assert_eq!(c.value(), "world");
        assert_eq!(c.get("&"), Some("world"));

        c.unset("&");
        assert_eq!(c.value(), "");
        assert_eq!(c.get("&.value"), Some(""));
    }

    #[test]
    fn raw_term_named_ampersand_cannot_shadow_focal_value() {
        let mut c = Cursor::default();
        c.set("&", "focal");
        c.terms.push(Term::new("&", "shadow"));

        assert_eq!(c.get("&"), Some("focal"));
        assert_eq!(c.get("&.value"), Some("focal"));
    }

    #[test]
    fn row_fields_include_focal_term_first() {
        let mut c = Cursor::default();
        c.set("&", "focal");
        c.set("NAME", "alpha");

        let fields = effect_runtime::v2::Row::fields(&c);
        assert_eq!(fields[0], ("&", "focal"));
        assert!(fields.contains(&("NAME", "alpha")));
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 2b  Interner — share Arc<str> heap for repeated values
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// The matcher emits a Cursor per match. File path strings repeat
// thousands of times across matches (avg ~80 matches per file in the
// $F($$$) workload). Without interning, each cursor allocates a fresh
// Arc<str> for the path. With interning, every cursor for the same
// file holds the SAME Arc<str> — an 8-byte pointer + atomic refcount
// bump — so the per-row heap cost drops from ~50 bytes (path content)
// to 8 bytes (pointer).
//
// Cost: HashMap lookup per intern call (one per emitted cursor for
// the path column). Amortized fast because hits dominate; misses
// happen only on first sighting.

#[derive(Default)]
pub struct Interner {
    /// Forward: content -> id.
    fwd: std::sync::RwLock<HashMap<Arc<str>, u32>>,
    /// Reverse: id -> content. id is index into this Vec.
    rev: std::sync::RwLock<Vec<Arc<str>>>,
}

impl Interner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }
    /// Return the canonical Arc<str> for `s`. Repeated calls with
    /// equal content return the same Arc clone.
    pub fn intern(&self, s: &str) -> Arc<str> {
        let id = self.intern_id(s);
        self.rev.read().unwrap()[id as usize].clone()
    }
    /// Return a stable u32 id for `s`. Cheaper than intern() when
    /// downstream storage is integer-keyed (DdStore arrangements).
    pub fn intern_id(&self, s: &str) -> u32 {
        if let Some(&id) = self.fwd.read().unwrap().get(s) {
            return id;
        }
        let mut fwd = self.fwd.write().unwrap();
        if let Some(&id) = fwd.get(s) {
            return id;
        }
        let mut rev = self.rev.write().unwrap();
        let id = rev.len() as u32;
        let arc: Arc<str> = Arc::from(s);
        fwd.insert(arc.clone(), id);
        rev.push(arc);
        id
    }
    /// Resolve id -> Arc<str>. Caller must use ids issued by this
    /// interner.
    pub fn lookup(&self, id: u32) -> Arc<str> {
        self.rev.read().unwrap()[id as usize].clone()
    }
    pub fn len(&self) -> usize {
        self.fwd.read().unwrap().len()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//   § 4   Telemetry — v3-shape spans, per-op-batch granularity
// ░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░
//
// One Telemetry per drive() call. Each op opens a span per batch via
// `tele.start("v4::Fs", Some(in_rows))` and closes it with `out_rows`.
// Summary groups by name and reports count / p50 / p95 / p99 / mean
// plus wall_window (latest_close − earliest_open) so concurrent batches
// across ops report aggregate throughput, not summed wall.

#[derive(Clone, Debug)]
pub struct Span {
    pub name: &'static str,
    pub start_ns: u64,
    pub wall_ns: u64,
    pub n_in: Option<u64>,
    pub n_out: Option<u64>,
    /// Bytes consumed by this span (sum of file sizes parsed, etc.).
    pub bytes_in: Option<u64>,
    /// Time spent in tree-sitter parse (sum across files inside batch).
    pub parse_ns: Option<u64>,
    /// Time spent in find_all + match collection.
    pub match_ns: Option<u64>,
    /// Process RSS at span close, KB. Sampled via getrusage.
    pub rss_kb_end: Option<u64>,
}

#[derive(Clone)]
pub struct Telemetry {
    inner: Arc<Mutex<Vec<Span>>>,
    epoch: Arc<Mutex<Instant>>,
}

impl Default for Telemetry {
    fn default() -> Self {
        Self::new()
    }
}

impl Telemetry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Vec::with_capacity(1 << 14))),
            epoch: Arc::new(Mutex::new(Instant::now())),
        }
    }
    pub fn start(&self, name: &'static str, n_in: Option<u64>) -> SpanOpen {
        let epoch = *self.epoch.lock().unwrap();
        let now = Instant::now();
        SpanOpen {
            name,
            started: now,
            start_ns: now.saturating_duration_since(epoch).as_nanos() as u64,
            n_in,
            bytes_in: None,
            parse_ns: None,
            match_ns: None,
            sink: self.inner.clone(),
        }
    }
    pub fn snapshot(&self) -> Vec<Span> {
        self.inner.lock().unwrap().clone()
    }
    pub fn drain(&self) -> Vec<Span> {
        std::mem::take(&mut *self.inner.lock().unwrap())
    }
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
        *self.epoch.lock().unwrap() = Instant::now();
    }
    pub fn report(&self) -> Vec<OpReport> {
        let spans = self.inner.lock().unwrap();
        let mut by: HashMap<&'static str, Vec<&Span>> = HashMap::new();
        for s in spans.iter() {
            by.entry(s.name).or_default().push(s);
        }
        let mut out: Vec<OpReport> = by
            .into_iter()
            .map(|(n, v)| OpReport::from_spans(n, &v))
            .collect();
        out.sort_by(|a, b| b.total_wall_ns.cmp(&a.total_wall_ns));
        out
    }
    pub fn summary(&self) -> String {
        let reports = self.report();
        let mut s = String::with_capacity(2048);
        // Row 1: timing + throughput
        s.push_str(&format!(
            "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
            "op", "batches", "p50", "p95", "p99", "mean", "wall", "MB", "MB/s",
        ));
        s.push_str(&format!(
            "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
            "-".repeat(20),
            "-------",
            "---",
            "---",
            "---",
            "----",
            "----",
            "--",
            "----",
        ));
        for r in &reports {
            let wall_s = r.wall_window_ns as f64 / 1e9;
            let mb = r.total_bytes_in.map(|b| b as f64 / 1_048_576.0);
            let mbs = match (mb, wall_s) {
                (Some(m), w) if w > 0.0 => Some(m / w),
                _ => None,
            };
            s.push_str(&format!(
                "{:<20} {:>7} {:>9} {:>9} {:>9} {:>9} {:>10} {:>9} {:>10}\n",
                short_name(r.name),
                r.count,
                fmt_ns(r.p50_ns),
                fmt_ns(r.p95_ns),
                fmt_ns(r.p99_ns),
                fmt_ns(r.mean_ns),
                fmt_ns(r.wall_window_ns),
                mb.map(|m| format!("{:.1}", m))
                    .unwrap_or_else(|| "—".into()),
                mbs.map(|m| format!("{:.1}", m))
                    .unwrap_or_else(|| "—".into()),
            ));
        }
        // Row 2: parse vs match split + RSS
        s.push('\n');
        s.push_str(&format!(
            "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
            "op", "parse_sum", "match_sum", "p/m", "rss_min", "rss_max", "rss_last",
        ));
        s.push_str(&format!(
            "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
            "-".repeat(20),
            "---------",
            "---------",
            "---",
            "-------",
            "-------",
            "--------",
        ));
        for r in &reports {
            let pm_ratio = match (r.total_parse_ns, r.total_match_ns) {
                (Some(p), Some(m)) if m > 0 => Some(p as f64 / m as f64),
                _ => None,
            };
            let mb_str = |kb: Option<u64>| {
                kb.map(|k| format!("{} MB", k / 1024))
                    .unwrap_or_else(|| "—".into())
            };
            s.push_str(&format!(
                "{:<20} {:>10} {:>10} {:>7} {:>10} {:>10} {:>10}\n",
                short_name(r.name),
                r.total_parse_ns.map(fmt_ns).unwrap_or_else(|| "—".into()),
                r.total_match_ns.map(fmt_ns).unwrap_or_else(|| "—".into()),
                pm_ratio
                    .map(|x| format!("{:.1}×", x))
                    .unwrap_or_else(|| "—".into()),
                mb_str(r.rss_kb_min),
                mb_str(r.rss_kb_max),
                mb_str(r.rss_kb_last),
            ));
        }
        s
    }
}

pub struct SpanOpen {
    name: &'static str,
    started: Instant,
    start_ns: u64,
    n_in: Option<u64>,
    bytes_in: Option<u64>,
    parse_ns: Option<u64>,
    match_ns: Option<u64>,
    sink: Arc<Mutex<Vec<Span>>>,
}

impl SpanOpen {
    pub fn set_bytes(&mut self, bytes: u64) {
        self.bytes_in = Some(bytes);
    }
    pub fn set_parse_ns(&mut self, ns: u64) {
        self.parse_ns = Some(ns);
    }
    pub fn set_match_ns(&mut self, ns: u64) {
        self.match_ns = Some(ns);
    }
    pub fn close(self, n_out: Option<u64>) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let rss_kb_end = Some(rss_peak_kb_now());
        let span = Span {
            name: self.name,
            start_ns: self.start_ns,
            wall_ns,
            n_in: self.n_in,
            n_out,
            bytes_in: self.bytes_in,
            parse_ns: self.parse_ns,
            match_ns: self.match_ns,
            rss_kb_end,
        };
        self.sink.lock().unwrap().push(span);
        std::mem::forget(self);
    }
}

impl Drop for SpanOpen {
    fn drop(&mut self) {
        let wall_ns = self.started.elapsed().as_nanos() as u64;
        let span = Span {
            name: self.name,
            start_ns: self.start_ns,
            wall_ns,
            n_in: self.n_in,
            n_out: None,
            bytes_in: self.bytes_in,
            parse_ns: self.parse_ns,
            match_ns: self.match_ns,
            rss_kb_end: Some(rss_peak_kb_now()),
        };
        if let Ok(mut v) = self.sink.lock() {
            v.push(span);
        }
    }
}

/// Push a synthetic span with a precomputed wall_ns. Useful when the
/// timed work happens on a non-async thread (e.g. timely worker) and
/// we forward accumulated nanoseconds across a channel boundary.
pub fn push_synthetic_span(t: &Telemetry, name: &'static str, wall_ns: u64, n_out: Option<u64>) {
    let epoch = *t.epoch.lock().unwrap();
    let now = Instant::now();
    let start_ns = now.saturating_duration_since(epoch).as_nanos() as u64;
    let span = Span {
        name,
        start_ns,
        wall_ns,
        n_in: None,
        n_out,
        bytes_in: None,
        parse_ns: None,
        match_ns: None,
        rss_kb_end: Some(rss_peak_kb_now()),
    };
    t.inner.lock().unwrap().push(span);
}

fn rss_peak_kb_now() -> u64 {
    unsafe {
        let mut u: libc::rusage = std::mem::zeroed();
        if libc::getrusage(libc::RUSAGE_SELF, &mut u) != 0 {
            return 0;
        }
        #[cfg(target_os = "macos")]
        {
            (u.ru_maxrss as u64) / 1024
        }
        #[cfg(not(target_os = "macos"))]
        {
            u.ru_maxrss as u64
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpReport {
    pub name: &'static str,
    pub count: usize,
    pub p50_ns: u64,
    pub p95_ns: u64,
    pub p99_ns: u64,
    pub mean_ns: u64,
    pub total_wall_ns: u64,
    pub wall_window_ns: u64,
    pub total_in: Option<u64>,
    pub total_out: Option<u64>,
    pub total_bytes_in: Option<u64>,
    pub total_parse_ns: Option<u64>,
    pub total_match_ns: Option<u64>,
    pub rss_kb_min: Option<u64>,
    pub rss_kb_max: Option<u64>,
    pub rss_kb_last: Option<u64>,
}

impl OpReport {
    fn from_spans(name: &'static str, spans: &[&Span]) -> Self {
        let count = spans.len();
        let mut walls: Vec<u64> = spans.iter().map(|s| s.wall_ns).collect();
        walls.sort_unstable();
        let p = |q: f64| -> u64 {
            if walls.is_empty() {
                return 0;
            }
            walls[((walls.len() - 1) as f64 * q).round() as usize]
        };
        let sum_wall: u64 = walls.iter().sum();
        let mean_ns = if count > 0 {
            sum_wall / count as u64
        } else {
            0
        };
        let earliest = spans.iter().map(|s| s.start_ns).min().unwrap_or(0);
        let latest = spans
            .iter()
            .map(|s| s.start_ns.saturating_add(s.wall_ns))
            .max()
            .unwrap_or(0);
        let mut total_in: Option<u64> = None;
        let mut total_out: Option<u64> = None;
        let mut total_bytes_in: Option<u64> = None;
        let mut total_parse_ns: Option<u64> = None;
        let mut total_match_ns: Option<u64> = None;
        let mut rss_kb_min: Option<u64> = None;
        let mut rss_kb_max: Option<u64> = None;
        let mut rss_kb_last: Option<u64> = None;
        for s in spans {
            if let Some(n) = s.n_in {
                total_in = Some(total_in.unwrap_or(0) + n);
            }
            if let Some(n) = s.n_out {
                total_out = Some(total_out.unwrap_or(0) + n);
            }
            if let Some(b) = s.bytes_in {
                total_bytes_in = Some(total_bytes_in.unwrap_or(0) + b);
            }
            if let Some(p) = s.parse_ns {
                total_parse_ns = Some(total_parse_ns.unwrap_or(0) + p);
            }
            if let Some(m) = s.match_ns {
                total_match_ns = Some(total_match_ns.unwrap_or(0) + m);
            }
            if let Some(r) = s.rss_kb_end {
                rss_kb_min = Some(rss_kb_min.map(|x| x.min(r)).unwrap_or(r));
                rss_kb_max = Some(rss_kb_max.map(|x| x.max(r)).unwrap_or(r));
                rss_kb_last = Some(r);
            }
        }
        Self {
            name,
            count,
            p50_ns: p(0.50),
            p95_ns: p(0.95),
            p99_ns: p(0.99),
            mean_ns,
            total_wall_ns: sum_wall,
            wall_window_ns: latest.saturating_sub(earliest),
            total_in,
            total_out,
            total_bytes_in,
            total_parse_ns,
            total_match_ns,
            rss_kb_min,
            rss_kb_max,
            rss_kb_last,
        }
    }
}

fn fmt_ns(ns: u64) -> String {
    if ns >= 1_000_000_000 {
        format!("{:.2}s", ns as f64 / 1e9)
    } else if ns >= 1_000_000 {
        format!("{:.1}ms", ns as f64 / 1e6)
    } else if ns >= 1_000 {
        format!("{:.1}µs", ns as f64 / 1e3)
    } else {
        format!("{}ns", ns)
    }
}
fn short_name(full: &'static str) -> String {
    full.rsplit("::").next().unwrap_or(full).to_string()
}

// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟
// ▜▛  § 5   Action / lineage counters (process-globals)              ▜▛
// ▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟▙▟

pub static GEN: AtomicU64 = AtomicU64::new(0);
pub static LIN: AtomicU64 = AtomicU64::new(0);

pub fn new_action(kind: ActionKind, parent: Option<(Gen, LineageId)>) -> Action {
    Action {
        gen: GEN.fetch_add(1, Ordering::SeqCst) + 1,
        parent,
        kind,
    }
}

pub fn new_lineage() -> LineageId {
    LIN.fetch_add(1, Ordering::SeqCst) + 1
}
