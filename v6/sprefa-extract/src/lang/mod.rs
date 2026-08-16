//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the ast-grep CST fallback. A `.rs` hits
//! `RustSource` (cst via ast-grep + type/call/df via syn); a `.go` hits
//! `GoSource` (cst via ast-grep + type/call/df via tree-sitter-go); a `.kt`/
//! `.kts` hits `KotlinSource` (cst via ast-grep + type/call/df via
//! tree-sitter-kotlin); a `.ts` hits `TsSource` (cst via ast-grep +
//! type/call/df via oxc); anything else with an ast-grep grammar falls to
//! `AstgrepSource` (cst-only).

pub mod astgrep;
pub mod dl6;
pub mod go;
pub mod kotlin;
pub mod markdown;
pub mod prolog;
pub mod python;
pub mod rust;
pub mod ts;

pub use astgrep::{
    query_patterns, AstCaptureFact, AstGrepParser, AstPatternQuery, AstgrepSource, CstProjector,
    SgRoot,
};
pub use dl6::DlSource;
pub use go::GoSource;
pub use kotlin::KotlinSource;
pub use markdown::MarkdownSource;
pub use prolog::PrologSource;
pub use python::PythonSource;
pub use rust::RustSource;
pub use ts::{CallProjector, DfProjector, OxcParser, TsSource, TypeProjector};

use crate::source::Source;

/// The first-match roster. Order matters: the lang-specific `Source`s precede the
/// ast-grep CST fallback (v5 `type_langs()` convention). RustSource is first so a
/// `.rs` routes to it, not the cst-only AstgrepSource; GoSource precedes
/// AstgrepSource so a `.go` routes to it, not the cst-only fallback.
/// KotlinSource precedes TsSource because `"x.kts".ends_with(".ts")` is true -
/// a `.kts` must route to kotlin, not ts (v5 `type_langs()` makes the same
/// order-dependent call, typegraph/mod.rs:488).
pub fn sources() -> &'static [&'static dyn Source] {
    &[
        &RustSource,
        &GoSource,
        &KotlinSource,
        &MarkdownSource,
        &PrologSource,
        &DlSource,
        &TsSource,
        &AstgrepSource,
    ]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}
