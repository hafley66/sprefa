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
