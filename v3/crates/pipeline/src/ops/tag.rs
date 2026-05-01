//! Alias shim: `tag` / `tag?` → `fact` / `fact?` (alias period).
//!
//! The user-facing op name is now `fact`. The registry registers `"fact"` and
//! `"fact?"` as the canonical names. `"tag"` and `"tag?"` are accepted by the
//! lowerer (see `registry.rs`) and emit diag `tag/renamed-to-fact`.
//!
//! Type aliases here preserve any external callers that reference `TagOp` /
//! `TagPredicateOp` / `TagArg` by name without requiring an immediate sweep
//! of downstream crates.

pub use super::fact::{FactArg as TagArg, FactOp as TagOp, FactPredicateOp as TagPredicateOp};
