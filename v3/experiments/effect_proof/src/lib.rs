//! Consumer crate for `effect_runtime`. Holds the demo effect modules
//! (original surface tests) and the three bench binaries. Framework
//! itself lives in the sibling `effect_runtime` crate.
//!
//! Nothing in this crate leaks back into the framework: the framework
//! stays neutral (no sqlite, no git, no ast-grep, no regex deps), and
//! every domain-specific concept lives here or in the bin targets.

pub mod effects;

// Re-export the framework entry points so demo tests can say
// `effect_proof::RtCtxBuilder` if they choose, or reach directly for
// `effect_runtime::RtCtxBuilder`. Both compile.
pub use effect_runtime::{
    Batcher, BoxFuture, CancellationToken, EffectKind, PureEffect, RtCtx, RtCtxBuilder,
};
