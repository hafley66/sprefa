//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the ast-grep CST fallback. A `.ts` hits
//! `TsSource` (cst via ast-grep + type/call/df via oxc); a `.rs` falls to
//! `AstgrepSource` (cst-only). Rust/Go `Source`s prepend in commits 5/6.

pub mod astgrep;
pub mod ts;

pub use astgrep::{AstGrepParser, AstgrepSource, CstProjector, SgRoot};
pub use ts::{CallProjector, DfProjector, OxcParser, TsSource, TypeProjector};

use crate::source::Source;

/// The first-match roster. Order matters: the lang-specific `Source` precedes the
/// ast-grep CST fallback (v5 `type_langs()` convention).
pub fn sources() -> &'static [&'static dyn Source] {
    &[&TsSource, &AstgrepSource]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}
