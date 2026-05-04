//! `Next` — carrier trait, with content identity baked in.
//!
//! Every value flowing through the queue must produce a stable
//! `[u8; 32]` content hash. The hash is the basis for `NextKey`
//! (queue-row identity), the cache key in `Memoize<C>`, and the
//! invalidation-event payload on the `EventBus`.
//!
//! No blanket impl: each carrier opts in. Cheap encodings are fine
//! (`blake3::hash(self.to_le_bytes())` for primitives), but the value
//! must be deterministic across runs and processes — we serialize it
//! into the queue and read it back after restart.
//!
//! The name borrows from RxJS observers (`observer.next(value)`):
//! `Next` is "the thing handed to the next render".

pub trait Next: Send + Sync + 'static {
    fn content_hash(&self) -> [u8; 32];
}

// Convenience impls for the common primitive carriers used in tests.

impl Next for i64 {
    fn content_hash(&self) -> [u8; 32] { blake3::hash(&self.to_le_bytes()).into() }
}

impl Next for u64 {
    fn content_hash(&self) -> [u8; 32] { blake3::hash(&self.to_le_bytes()).into() }
}

impl Next for String {
    fn content_hash(&self) -> [u8; 32] { blake3::hash(self.as_bytes()).into() }
}
