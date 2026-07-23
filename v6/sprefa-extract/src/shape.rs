//! S1 atoms: one coordinate, one identity, one family tag.
//!
//! The normalization this layer enforces (vs v5): a fact's WHERE is one byte
//! span, a fact's WHAT is one typed kind, a fact's identity is its span (never a
//! minted coordinate string). v5 had four span shapes, three kind reps, and
//! split node identity (`mint_sym` / `NodeIdx` / `WhereBytes`); those are
//! deleted here, not patched.
//!
//! All types are content-LOCAL. `NameId` resolves through this module's `Strings`
//! (extract's own per-file arena interner); the engine seam maps `NameId ->`
//! the store dictionary at the boundary. `BlobHash` is the content key (matches
//! `store::files.content_hash`); commit 1 declares it but does not compute it
//! (the cache lands with the dispatch lab).

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A byte span into the blob the extractor was handed. THE coordinate. Line/col
/// are never stored; the engine derives them from the file bytes when needed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub len: u32,
}

impl Span {
    pub const fn empty() -> Self {
        Self { start: 0, len: 0 }
    }
    /// Synthetic node identity for things with no real span (a whole-file
    /// module). Identity-stable, span-meaningless.
    pub const fn anchor(at: u32) -> Self {
        Self { start: at, len: 0 }
    }
    pub const fn end(self) -> u32 {
        self.start + self.len
    }
}

/// Content key: blake3 truncated to 16 raw bytes (store `files.content_hash`).
/// Two byte-identical blobs anywhere in the corpus share ONE extraction.
/// Commit 1 declares the type; hashing lands with the content-keyed cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BlobHash(pub [u8; 16]);

/// Extract's own arena-interned string id. Dense u32 into the per-file `Strings`
/// table. Names, grammar kinds, specifiers all intern here. The engine seam maps
/// a `NameId` into the store dictionary; extract never mints a qualified sym.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NameId(pub u32);

/// Local index into one file's node vec. Edges reference nodes by this during
/// extraction; the wire flattens `NodeRef -> Span` so the local id never crosses
/// the seam or the stdout stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct NodeRef(pub u32);

/// The family discriminant at the FLAT seam only (the wire, the ratchet key).
/// In-memory types are per-family (`Node<F>`); this tag appears when the family
/// is flattened onto a row, never inside the generic types.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FamilyTag {
    Df,
    Call,
    Type,
    Module,
    Cst,
}

/// The per-file string interner backing every `NameId`. One per extraction; the
/// dispatch creates it, passes `&mut` to each projector, and keeps it so the
/// wire flatten can resolve `NameId -> &str`. Dedups on insert.
#[derive(Default)]
pub struct Strings {
    map: HashMap<String, NameId>,
    names: Vec<String>,
}

impl Strings {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern `s`, returning a stable `NameId`. Byte-identical strings share one
    /// id (the dedup that keeps the dictionary small).
    pub fn intern(&mut self, s: &str) -> NameId {
        if let Some(id) = self.map.get(s) {
            return *id;
        }
        let id = NameId(self.names.len() as u32);
        self.map.insert(s.to_string(), id);
        self.names.push(s.to_string());
        id
    }

    pub fn lookup(&self, id: NameId) -> &str {
        &self.names[id.0 as usize]
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

impl fmt::Display for NameId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "NameId({})", self.0)
    }
}
