//! `Codec` — byte (de)serialization for carriers stored in SqliteQueue.
//!
//! Separate from `Next::content_hash`: hash is identity, codec is
//! transport. A carrier may have a content hash and not yet be
//! persistable (in-RAM-only test types) or vice versa, so the bounds
//! are decoupled. `SqliteQueue<N>` requires `N: Next + Codec`;
//! `MemQueue<N>` only requires `N: Next`.
//!
//! Encoding is opaque to the runtime — whatever bytes round-trip back
//! to an equal value. blake3-of-bytes need not match `content_hash`.

pub trait Codec: Sized {
    fn encode(&self) -> Vec<u8>;
    fn decode(bytes: &[u8]) -> Self;
}

// Convenience impls matching `next.rs`.

impl Codec for i64 {
    fn encode(&self) -> Vec<u8> { self.to_le_bytes().to_vec() }
    fn decode(bytes: &[u8]) -> Self {
        let mut a = [0u8; 8]; a.copy_from_slice(&bytes[..8]); i64::from_le_bytes(a)
    }
}

impl Codec for u64 {
    fn encode(&self) -> Vec<u8> { self.to_le_bytes().to_vec() }
    fn decode(bytes: &[u8]) -> Self {
        let mut a = [0u8; 8]; a.copy_from_slice(&bytes[..8]); u64::from_le_bytes(a)
    }
}

impl Codec for String {
    fn encode(&self) -> Vec<u8> { self.as_bytes().to_vec() }
    fn decode(bytes: &[u8]) -> Self { std::str::from_utf8(bytes).unwrap().to_string() }
}
