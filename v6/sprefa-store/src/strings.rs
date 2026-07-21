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
