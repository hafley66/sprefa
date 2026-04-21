//! sprefa::diagnostic — re-export shim over `spine::diagnostic`.
//!
//! `Diagnostic` / `Renderer` traits and their concrete shapes live in
//! spine. Ops and runtime code keep using `crate::diagnostic::*` paths
//! unchanged.
pub use spine::diagnostic::*;
