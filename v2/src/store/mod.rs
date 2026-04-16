//! `store` — persistence + mutation cache surface.
//!
//! # Role
//! Single-trait interface for SQLite-backed durability. Phase 1 ships the
//! trait surface and types only; Phase 2 wires the SQLite impl.
//!
//! # Ownership + lifecycle
//! Implementations are held as `Arc<dyn Store>` on `OpCtx`, `DocSession`,
//! the CLI binary, and the future evaluator. Reference-counted; the
//! DocSession and evaluator share the same pool when colocated.
//!
//! # Who mutates
//! `flush_batch` and `record_effect` are the only write methods. All
//! other methods are read-only. Concurrent calls from multiple ops are
//! expected; impls must be `Send + Sync`.
//!
//! # Failure modes
//! Every method returns `Result<_, StoreErr>`. `StoreErr` implements
//! `Diagnostic` so callers can bubble errors into the run's diagnostic
//! stream via `RunEvent::Diag`.

pub mod _0_types;
pub mod _1_trait;

pub use _0_types::*;
pub use _1_trait::*;
