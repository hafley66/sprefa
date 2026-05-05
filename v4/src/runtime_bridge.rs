//! Bridge: v4 `Cursor` ↔ `effect_runtime::v2`.
//!
//! Implements `Next` and `Codec` for `Cursor` so a sprefa pipe can be
//! built directly on the v2 runtime (`Component<Next = Cursor>` +
//! `MemQueue<Cursor>`). The encoding mirrors `cursor_codec` — one
//! source of bytes for hashing and persistence.

use crate::Cursor;
use crate::cursor_codec;

impl effect_runtime::v2::Next for Cursor {
    // PERF TODO: cache the hash on the Cursor (Cell<Option<[u8;32]>>
    // or OnceCell) — Memoize calls content_hash() once per render plus
    // once per cache lookup, and the encode-then-blake3 path is
    // proportional to terms.len(). For hot Cursors flowing through
    // multiple Memoize layers this re-hashes. Caching is defensive
    // (immutable post-construction) but requires Cursor::set to
    // invalidate. Independent of the v2 runtime.
    fn content_hash(&self) -> [u8; 32] {
        let bytes = cursor_codec::encode(self);
        *blake3::hash(&bytes).as_bytes()
    }
}

impl effect_runtime::v2::Codec for Cursor {
    fn encode(&self) -> Vec<u8> { cursor_codec::encode(self) }
    fn decode(bytes: &[u8]) -> Self {
        cursor_codec::decode(bytes).expect("valid cursor codec bytes")
    }
}
