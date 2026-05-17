//! Phase 1: one source clock for everything externally mutable.
//!
//! `table_version` / `bump_table_version` were a per-table monotone
//! counter living in the fact store. They subsume into ONE clock keyed
//! by `SourceId` so a file edit, an LSP buffer change, and a
//! rule/fact-table write all bump the same kind of generation. The
//! authoritative state is the persisted `SOURCE_GEN(source_id, gen)`
//! table; a process-local map fronts it as the hot tier.
//!
//! `SourceId = blake3("src" ++ canonical_uri)`. No new id-family math:
//! the bytes are a blake3 digest folded the same way every other
//! Layer-0b id is.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use effect_runtime::v2::FactStore;

use crate::Cursor;

/// Persisted clock table. One row per touched source.
pub const SOURCE_GEN_TABLE: &str = "_source_gen";

/// Anything externally mutable that an op reads: a file, an LSP
/// buffer, or a rule/fact table. Identity is `blake3("src" ++ uri)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SourceId(pub [u8; 32]);

impl SourceId {
    /// `blake3("src" ++ canonical_uri)`.
    pub fn of(canonical_uri: &str) -> Self {
        let mut h = blake3::Hasher::new();
        h.update(b"src");
        h.update(canonical_uri.as_bytes());
        SourceId(*h.finalize().as_bytes())
    }

    /// A rule/fact table is itself a source. Canonical URI is
    /// `table:<name>` so a write to it bumps its gen the same way a
    /// file edit does.
    pub fn for_table(table: &str) -> Self {
        Self::of(&format!("table:{table}"))
    }

    /// A file path source. Canonical URI is `file:<path>`.
    pub fn for_file(path: &str) -> Self {
        Self::of(&format!("file:{path}"))
    }

    /// An LSP buffer source. Canonical URI is `buffer:<uri>`.
    pub fn for_buffer(uri: &str) -> Self {
        Self::of(&format!("buffer:{uri}"))
    }

    /// Lowercase hex of the digest — the `source_id` PK column value.
    pub fn hex(&self) -> String {
        let mut s = String::with_capacity(64);
        for b in &self.0 {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

/// Monotone per-source counter. Bumped only by the event layer
/// (fs-watch / lsp / fact-table write paths).
pub trait SourceClock: Send + Sync {
    /// Current generation for `s`. 0 if the source has never been seen.
    fn current_gen(&self, s: SourceId) -> u64;
    /// Increment and return the new generation. Event layer only.
    fn bump(&self, s: SourceId) -> u64;
}

/// `SOURCE_GEN`-backed clock. Hot tier is a process-local map; cold
/// tier is the persisted `_source_gen` fact table so a generation
/// survives a restart and the SQL batch cache key stays correct
/// across process boundaries.
pub struct FactStoreClock {
    store: Arc<dyn FactStore<Cursor>>,
    hot: Mutex<HashMap<[u8; 32], u64>>,
}

impl FactStoreClock {
    pub fn new(store: Arc<dyn FactStore<Cursor>>) -> Arc<Self> {
        store.declare(SOURCE_GEN_TABLE, &["source_id", "gen"]);
        Arc::new(Self {
            store,
            hot: Mutex::new(HashMap::new()),
        })
    }

    fn cold_gen(&self, s: SourceId) -> u64 {
        self.store
            .read_where(SOURCE_GEN_TABLE, "source_id", &s.hex())
            .iter()
            .filter_map(|row| row.get("gen").and_then(|g| g.parse::<u64>().ok()))
            .max()
            .unwrap_or(0)
    }

    fn persist(&self, s: SourceId, gen: u64) {
        // Delete-then-insert keeps a single live row per source. The
        // store mints `_id` from the row content; the source_id PK is
        // explicit so the cache key folds a stable value.
        self.store
            .delete_matching(SOURCE_GEN_TABLE, &[("source_id", &s.hex())]);
        let mut row = Cursor::default();
        row.set("source_id", s.hex());
        row.set("gen", gen.to_string());
        self.store.insert(SOURCE_GEN_TABLE, Arc::new(row));
    }
}

impl SourceClock for FactStoreClock {
    fn current_gen(&self, s: SourceId) -> u64 {
        if let Some(g) = self.hot.lock().unwrap().get(&s.0).copied() {
            return g;
        }
        let g = self.cold_gen(s);
        self.hot.lock().unwrap().insert(s.0, g);
        g
    }

    fn bump(&self, s: SourceId) -> u64 {
        let mut hot = self.hot.lock().unwrap();
        let cur = match hot.get(&s.0).copied() {
            Some(g) => g,
            None => self.cold_gen(s),
        };
        let next = cur.saturating_add(1);
        hot.insert(s.0, next);
        drop(hot);
        self.persist(s, next);
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use effect_runtime::v2::MemFactStore;

    fn store() -> Arc<dyn FactStore<Cursor>> {
        Arc::new(MemFactStore::<Cursor>::new())
    }

    #[test]
    fn unseen_source_is_gen_zero() {
        let clock = FactStoreClock::new(store());
        assert_eq!(clock.current_gen(SourceId::for_file("/a.rs")), 0);
    }

    #[test]
    fn bump_is_monotone_and_persists() {
        let s = store();
        let clock = FactStoreClock::new(s.clone());
        let f = SourceId::for_file("/a.rs");
        assert_eq!(clock.bump(f), 1);
        assert_eq!(clock.bump(f), 2);
        assert_eq!(clock.current_gen(f), 2);

        // Cold tier holds it: a fresh clock over the same store reads 2.
        let clock2 = FactStoreClock::new(s);
        assert_eq!(clock2.current_gen(f), 2);
    }

    #[test]
    fn file_and_table_sources_are_independent() {
        let clock = FactStoreClock::new(store());
        let f = SourceId::for_file("/a.rs");
        let t = SourceId::for_table("imports");
        clock.bump(f);
        assert_eq!(clock.current_gen(f), 1);
        assert_eq!(clock.current_gen(t), 0);
        clock.bump(t);
        assert_eq!(clock.current_gen(t), 1);
        assert_eq!(clock.current_gen(f), 1);
    }

    #[test]
    fn distinct_uris_have_distinct_ids() {
        assert_ne!(
            SourceId::for_file("/a.rs").0,
            SourceId::for_table("a.rs").0
        );
        assert_ne!(
            SourceId::for_file("/a.rs").0,
            SourceId::for_file("/b.rs").0
        );
    }
}
