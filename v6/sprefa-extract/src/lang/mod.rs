//! The language roster. First-match (v5 `type_langs()`, typegraph/mod.rs:491):
//! the lang-specific `Source` precedes the ast-grep CST fallback. A `.rs` hits
//! `RustSource` (cst via ast-grep + type/call/df via syn); a `.go` hits
//! `GoSource` (cst via ast-grep + type/call/df via tree-sitter-go); a `.kt`/
//! `.kts` hits `KotlinSource` (cst via ast-grep + type/call/df via
//! tree-sitter-kotlin); a `.py`/`.pyi` hits `PythonSource` (cst via ast-grep +
//! type/call/df via tree-sitter-python); a `.ts` hits `TsSource` (cst via ast-grep +
//! type/call/df via oxc); anything else with an ast-grep grammar falls to
//! `AstgrepSource` (cst-only).

#[path = "1_ast_rule.rs"]
pub mod ast_rule;
pub mod astgrep;
pub mod data;
pub mod dl6;
pub mod extract_lang;
pub mod fact;
pub mod go;
pub mod go_modules;
pub mod kotlin;
pub mod kotlin_rehome;
pub mod kotlin_rename;
pub mod markdown;
pub mod prolog;
pub mod python;
pub mod rust;
pub mod rust_checker;
#[cfg(feature = "rust-checker")]
mod rust_checker_ra;
pub mod rust_mbe;
pub mod rust_modules;
pub mod rust_receivers;
pub mod rust_rehome;
pub mod rust_rename;
pub mod rust_docs;
pub mod rust_scip_macros;
pub mod rust_type_edges;
pub mod rust_type_refs;
pub mod ts;
pub mod ts_paths;
pub mod ts_receivers;
pub mod ts_rehome;
pub mod ts_rename;
pub mod ts_resolve;

pub use ast_rule::{
    decode_ast_rule_yaml, query_ast_rule, query_ast_rule_with_content, AstRule, AstRuleCapture,
    AstRuleError, AstRuleMatch, AstRuleMutationProposal, AstRuleRequest, NamedAstRule, StopBy,
};
pub use astgrep::{
    query_patterns, AstCaptureFact, AstGrepParser, AstPatternQuery, AstgrepSource, CstProjector,
    SgRoot,
};
pub use data::DataSource;
pub use dl6::DlSource;
pub use extract_lang::ExtractLang;
pub use fact::{
    dl6_db_path, open_dl6_readonly, open_readonly, FactError, FactMatcher, FactSet,
    DL6_DB_RELATIVE_PATH,
};
pub use go::GoSource;
pub use kotlin::KotlinSource;
pub use markdown::MarkdownSource;
pub use prolog::PrologSource;
pub use python::PythonSource;
pub use rust::RustSource;
pub use ts::{
    ts_specifiers, CallProjector, DfProjector, OxcParser, TsSource, TsSpecifier, TypeProjector,
};
pub use ts_rehome::{build_paths, compiled_spellings, BuildPaths};
pub use ts_resolve::{respell, TsResolver};

use crate::source::Source;
use crate::types::{RehomeArm, Rename};

/// The first-match roster. Order matters: the lang-specific `Source`s precede the
/// ast-grep CST fallback (v5 `type_langs()` convention). RustSource is first so a
/// `.rs` routes to it, not the cst-only AstgrepSource; GoSource precedes
/// AstgrepSource so a `.go` routes to it, not the cst-only fallback.
/// KotlinSource precedes TsSource because `"x.kts".ends_with(".ts")` is true -
/// a `.kts` must route to kotlin, not ts (v5 `type_langs()` makes the same
/// order-dependent call, typegraph/mod.rs:488).
/// DataSource precedes AstgrepSource so a `.json`/`.yaml` reaches the data plane;
/// it delegates its own cst plane back to AstgrepSource, so no row is lost.
pub fn sources() -> &'static [&'static dyn Source] {
    &[
        &RustSource,
        &GoSource,
        &KotlinSource,
        &MarkdownSource,
        &PrologSource,
        &PythonSource,
        &DataSource,
        &DlSource,
        &TsSource,
        &AstgrepSource,
    ]
}

/// The first `Source` whose `matches(path)` is true, else None.
pub fn source_for(path: &str) -> Option<&'static dyn Source> {
    sources().iter().copied().find(|src| src.matches(path))
}

/// The `Rehome` roster: one impl per language `extract move` can rehome, in
/// `sources()` order. A language with no impl here is a named stop, never a
/// `match` arm in the move core.
pub fn rehomes() -> &'static [RehomeArm] {
    const ROSTER: [RehomeArm; 4] = [
        RehomeArm {
            core: &RustSource,
            manifests: Some(&RustSource),
            shim: None,
            text_spellings: None,
            plan_check: Some(&RustSource),
        },
        RehomeArm {
            core: &KotlinSource,
            manifests: None,
            shim: None,
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &PrologSource,
            manifests: None,
            shim: Some(&PrologSource),
            text_spellings: None,
            plan_check: None,
        },
        RehomeArm {
            core: &TsSource,
            manifests: Some(&TsSource),
            shim: None,
            text_spellings: Some(&TsSource),
            plan_check: None,
        },
    ];
    &ROSTER
}

/// The `Rehome` that owns `path`, under the SAME first-match law `sources()`
/// states: `"x.kts".ends_with(".ts")` is true, so `TsSource` matches a kotlin
/// script too and only `source_for`'s own winner may claim it.
pub fn rehome_for(path: &str) -> Option<&'static RehomeArm> {
    let owner = source_for(path)?.name();
    rehomes().iter().find(|arm| arm.name() == owner)
}

/// The `Rename` roster, in `sources()` order. Membership is "has a scope plane
/// with exact identifier spans", a different question from `rehomes()`'s.
pub fn renames() -> &'static [&'static dyn Rename] {
    &[&TsSource, &RustSource, &KotlinSource, &PrologSource]
}

/// The `Rename` that owns `path`, under the SAME first-match law `rehome_for`
/// states: only `source_for`'s own winner may claim a path.
pub fn rename_for(path: &str) -> Option<&'static dyn Rename> {
    let owner = source_for(path)?.name();
    renames().iter().copied().find(|arm| arm.name() == owner)
}
