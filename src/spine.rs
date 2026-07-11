use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RepoId(pub u32);

impl RepoId {
    pub const SYNTHETIC: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RevId(pub u32);

impl RevId {
    pub const SYNTHETIC: Self = Self(0);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u64);

impl FileId {
    pub const SYNTHETIC: Self = Self(0);

    pub fn of_bytes(content: &[u8]) -> Self {
        if content.is_empty() {
            return Self::SYNTHETIC;
        }
        Self(hash64(content))
    }

    pub fn from_content_address(hash: &str, size: i64) -> Option<Self> {
        if size == 0 {
            return Some(Self::SYNTHETIC);
        }
        if !matches!(hash.len(), 40 | 64) || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let id = u64::from_str_radix(&hash[..16], 16).ok()?;
        if id == 0 {
            Some(Self::SYNTHETIC)
        } else {
            Some(Self(id))
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StringId(pub u64);

impl StringId {
    pub const EMPTY: Self = Self(0);

    pub fn of(text: &str) -> Self {
        if text.is_empty() {
            return Self::EMPTY;
        }
        Self(hash64(text.as_bytes()))
    }

    /// The id as it lives in SQLite INTEGER cells: the u64 bit pattern
    /// reinterpreted as i64. `_strings.id` and every `sym`-typed rel column
    /// store this value, so joins and literal filters are single-word integer
    /// compares. Display stays the decimal u64 (debug surfaces only).
    pub fn sqlite(self) -> i64 {
        self.0 as i64
    }

    pub fn from_sqlite(v: i64) -> Self {
        Self(v as u64)
    }
}

/// A validated, ready-to-store StringId for a `sym`-typed rel column. The ONLY
/// way to get one is `SymSink::sym`, which queues the (id, text) pair for the
/// batched `_strings` flush — so a Sym can never land in a row without its
/// text being interned alongside it (the turnkey emit-side API: no more
/// open-coded `StringId::of(text).sqlite()` at each call site). Deliberately
/// carries no `Display`/`to_string`: a Sym must never leak into a TEXT column
/// (that would store the decimal id as if it were the text itself).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Sym(StringId);

impl Sym {
    /// The i64 cell value for a `sym` column's `Value::Int`.
    pub fn cell(self) -> i64 {
        self.0.sqlite()
    }
}

/// Collects (id, text) pairs queued by `SymSink::sym` across one refresh
/// pass. `Db::flush_syms` drains it into ONE batched `_strings` insert — the
/// collect-then-flush shape every other spine write already follows. A debug
/// build panics if the sink is dropped with pending interns never flushed
/// (the N+1 law's silent-loss twin: better to panic loud than let interned
/// text for an already-written id vanish).
#[derive(Default)]
pub struct SymSink {
    pending: Vec<(StringId, String)>,
    flushed: bool,
}

impl SymSink {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `text`, queueing it for the next flush, and return its Sym.
    /// Empty text hashes to `StringId::EMPTY` (the sentinel) and is not
    /// queued — nothing to intern, the sentinel `_strings` row already covers it.
    pub fn sym(&mut self, text: &str) -> Sym {
        let id = StringId::of(text);
        if !text.is_empty() {
            self.pending.push((id, text.to_string()));
        }
        Sym(id)
    }

    /// Drain every queued (id, text) pair. Called once by `Db::flush_syms`;
    /// not meant to be called directly by refresh code.
    pub fn drain(&mut self) -> Vec<(StringId, String)> {
        self.flushed = true;
        std::mem::take(&mut self.pending)
    }
}

#[cfg(debug_assertions)]
impl Drop for SymSink {
    fn drop(&mut self) {
        if !self.flushed && !self.pending.is_empty() {
            panic!(
                "SymSink dropped with {} pending intern(s) never flushed — \
                 call Db::flush_syms(&mut sink) before the sink goes out of scope",
                self.pending.len()
            );
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RefId(pub u64);

impl RefId {
    pub const SYNTHETIC: Self = Self(0);

    pub fn of_coord(c: Coord) -> Self {
        if c == Coord::default() {
            return Self::SYNTHETIC;
        }
        let mut h = blake3::Hasher::new();
        h.update(&c.repo.0.to_be_bytes());
        h.update(&c.rev.0.to_be_bytes());
        h.update(&c.file.0.to_be_bytes());
        h.update(&c.lo.to_be_bytes());
        h.update(&c.hi.to_be_bytes());
        Self(first_u64(h.finalize().as_bytes()))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WhereBytesId(pub u64);

impl WhereBytesId {
    pub const SYNTHETIC: Self = Self(0);

    pub fn of(w: WhereBytes) -> Self {
        if w == WhereBytes::default() {
            return Self::SYNTHETIC;
        }
        if w.string == StringId::EMPTY {
            return Self(RefId::of_coord(w.into()).0);
        }
        let mut h = blake3::Hasher::new();
        h.update(&w.string.0.to_be_bytes());
        h.update(&w.repo.0.to_be_bytes());
        h.update(&w.rev.0.to_be_bytes());
        h.update(&w.file.0.to_be_bytes());
        h.update(&w.lo.to_be_bytes());
        h.update(&w.hi.to_be_bytes());
        Self(first_u64(h.finalize().as_bytes()))
    }

    pub fn coord_only(w: WhereBytes) -> Self {
        Self(RefId::of_coord(w.into()).0)
    }

    /// Like `of`, but folds the source `(repo, path)` into the row identity so
    /// two byte-identical files keep distinct located rows. Two cases collapse
    /// without this: re-export stubs / generated shims that share bytes within a
    /// repo (the `path` axis), and two config repos that share a path with
    /// identical content (the `repo` axis). Either collapse loses the second
    /// row on `INSERT OR IGNORE` and misfires `retract_paths`, which prunes by
    /// `(repo, path)`. The sentinel is preserved.
    pub fn of_located(w: WhereBytes, repo: &str, path: &str) -> Self {
        let base = Self::of(w);
        if base == Self::SYNTHETIC {
            return base;
        }
        let mut h = blake3::Hasher::new();
        h.update(&base.0.to_be_bytes());
        h.update(repo.as_bytes());
        h.update(&[0]); // separator: (repo="ab", path="") != (repo="a", path="b")
        h.update(path.as_bytes());
        Self(first_u64(h.finalize().as_bytes()))
    }

    /// Salt a located id by a discriminator (a CST node `kind`). Two tree-sitter
    /// nodes that share `(file, lo, hi)` but differ in kind — a wrapper and its
    /// sole child — must NOT collapse to one id, or innermost-containment merges
    /// them. The salt only perturbs the id; the underlying `_where_bytes` row
    /// still carries the RAW slice's StringId, so `ref(id, sid, ..)` ->
    /// `string(sid, text, ..)` resolves to the raw source bytes. The sentinel is
    /// preserved.
    pub fn salted(self, kind: &str) -> Self {
        if self == Self::SYNTHETIC {
            return self;
        }
        let mut h = blake3::Hasher::new();
        h.update(&self.0.to_be_bytes());
        h.update(&[1]); // separator distinct from of_located's [0]
        h.update(kind.as_bytes());
        Self(first_u64(h.finalize().as_bytes()))
    }
}

impl From<RefId> for WhereBytesId {
    fn from(r: RefId) -> Self {
        Self(r.0)
    }
}

impl From<WhereBytesId> for RefId {
    fn from(r: WhereBytesId) -> Self {
        Self(r.0)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Coord {
    pub repo: RepoId,
    pub rev: RevId,
    pub file: FileId,
    pub lo: u32,
    pub hi: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
            file: c.file,
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
            file: w.file,
            lo: w.lo,
            hi: w.hi,
        }
    }
}

pub const ZERO_HASH_HEX: &str = "0000000000000000000000000000000000000000000000000000000000000000";

pub fn content_hash_hex(content: &[u8]) -> String {
    blake3::hash(content).to_hex().to_string()
}

pub fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn hash64(content: &[u8]) -> u64 {
    first_u64(blake3::hash(content).as_bytes())
}

fn first_u64(bytes: &[u8; 32]) -> u64 {
    u64::from_be_bytes(
        bytes[..8]
            .try_into()
            .expect("blake3 hash has at least 8 bytes"),
    )
}

macro_rules! display_id {
    ($ty:ty) => {
        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

display_id!(RepoId);
display_id!(RevId);
display_id!(FileId);
display_id!(StringId);
display_id!(RefId);
display_id!(WhereBytesId);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sentinels_are_zero() {
        assert_eq!(RepoId::SYNTHETIC.0, 0);
        assert_eq!(RevId::SYNTHETIC.0, 0);
        assert_eq!(FileId::SYNTHETIC.0, 0);
        assert_eq!(StringId::EMPTY.0, 0);
        assert_eq!(RefId::SYNTHETIC.0, 0);
        assert_eq!(WhereBytesId::SYNTHETIC.0, 0);
        assert_eq!(FileId::of_bytes(b""), FileId::SYNTHETIC);
        assert_eq!(StringId::of(""), StringId::EMPTY);
        assert_eq!(RefId::of_coord(Coord::default()), RefId::SYNTHETIC);
        assert_eq!(
            WhereBytesId::of(WhereBytes::default()),
            WhereBytesId::SYNTHETIC
        );
    }

    #[test]
    fn ids_are_content_derived_and_stable() {
        assert_eq!(StringId::of("alpha"), StringId::of("alpha"));
        assert_ne!(StringId::of("alpha"), StringId::of("beta"));
        assert_eq!(FileId::of_bytes(b"alpha"), FileId::of_bytes(b"alpha"));
        assert_ne!(FileId::of_bytes(b"alpha"), FileId::of_bytes(b"beta"));
        assert_eq!(
            FileId::from_content_address(&content_hash_hex(b"alpha"), 5),
            Some(FileId::of_bytes(b"alpha"))
        );
        assert_eq!(
            FileId::from_content_address("0123456789abcdef0123456789abcdef01234567", 12),
            Some(FileId(0x0123456789abcdef))
        );
        assert_eq!(FileId::from_content_address("not-a-hash", 1), None);
        assert_eq!(
            FileId::from_content_address(&content_hash_hex(b""), 0),
            Some(FileId::SYNTHETIC)
        );

        let coord = Coord {
            repo: RepoId(1),
            rev: RevId(2),
            file: FileId(3),
            lo: 100,
            hi: 200,
        };
        assert_eq!(RefId::of_coord(coord), RefId::of_coord(coord));
        assert_ne!(
            RefId::of_coord(coord),
            RefId::of_coord(Coord { lo: 101, ..coord })
        );
    }

    #[test]
    fn where_bytes_empty_string_keeps_coord_identity() {
        let w = WhereBytes {
            string: StringId::EMPTY,
            repo: RepoId(1),
            rev: RevId(2),
            file: FileId(3),
            lo: 10,
            hi: 20,
        };
        assert_eq!(WhereBytesId::of(w), WhereBytesId::coord_only(w));
        assert_eq!(
            WhereBytesId::of(w),
            WhereBytesId::from(RefId::of_coord(w.into()))
        );

        let located_text = WhereBytes {
            string: StringId::of("needle"),
            ..w
        };
        assert_ne!(
            WhereBytesId::of(located_text),
            WhereBytesId::coord_only(located_text)
        );
    }

    #[test]
    fn normalize_strips_punctuation_and_lowercases_ascii() {
        assert_eq!(normalize("AuthService"), "authservice");
        assert_eq!(normalize("auth-service"), "authservice");
        assert_eq!(normalize("auth.service"), "authservice");
    }
}
