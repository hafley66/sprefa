//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the ast-grep CST fallback. A `.rs` hits
//! `RustSource` (cst via ast-grep + type/call/df via syn); a `.ts` hits
//! `TsSource` (cst via ast-grep + type/call/df via oxc); anything else with an
//! ast-grep grammar falls to `AstgrepSource` (cst-only).

pub mod astgrep;
pub mod rust;
pub mod ts;

pub use astgrep::{AstGrepParser, AstgrepSource, CstProjector, SgRoot};
pub use rust::RustSource;
pub use ts::{CallProjector, DfProjector, OxcParser, TsSource, TypeProjector};

use crate::source::Source;

/// The first-match roster. Order matters: the lang-specific `Source`s precede the
/// ast-grep CST fallback (v5 `type_langs()` convention). RustSource is first so a
/// `.rs` routes to it, not the cst-only AstgrepSource.
pub fn sources() -> &'static [&'static dyn Source] {
    &[&RustSource, &TsSource, &AstgrepSource]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}
