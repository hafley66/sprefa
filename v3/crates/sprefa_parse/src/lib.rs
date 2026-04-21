//! sprefa_parse — the parse half of sprefa v3.
//!
//! What lives here: source-bytes → AST. Nothing else. No runtime, no I/O,
//! no Op trait, no tokio, no sqlite, no git2. One dependency: std.
//!
//! Three modules, three jobs:
//!   - `site`  — `ParseSite` + `ParseSeg`, the compile-time coordinate the
//!                parser stamps on every AST node.
//!   - `ast`   — the shape the parser hands back: `OpInvocation`, slot
//!                types, `CrossRefOccurrence`, `BraceMode`, `GrammarRef`.
//!   - `parse` — `host_parse` and friends. Pure functions over strings.
//!
//! Who depends on us: the `sprefa` runtime crate. It lowers our
//! `OpInvocation` into its `Pipeline` enum, hangs `ParseSite` on cursors
//! and diagnostics, and re-exports our types through `crate::parse::*` and
//! `crate::types::ParseSite` so existing callers keep working.
//!
//! Why separate crate: force the boundary. If a "parse" function ever
//! reaches for `tokio` or `git2`, the compiler refuses. The contract is
//! physical, not documentary.

pub mod site;
pub mod ast;
pub mod parse;

pub use site::{ParseSite, ParseSeg};
pub use ast::{
    OpInvocation, BracketSlot, ParenSlot, BraceSlot, BraceMode,
    CrossRefOccurrence, GrammarRef,
};
pub use parse::*;
