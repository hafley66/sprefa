//! The Rust extractor arm: syn front-end for type/call/df/const, ast-grep for cst.
//! Mirrors TsSource (same shape, different front-end): cst via ast-grep's rust
//! grammar + one `syn::parse_file` feeding the type/call/df/const projections.
//!
//! Commit A (skeleton): RustSource wires cst via ast-grep + a syn parse, with the
//! type/call/df projections stubbed empty. Commits B/C/D port
//! `rust_entities_from` / `rust_const_values_from`, `rust_call_defs_from` /
//! `rust_call_sites_from`, and `rust_dataflow_from` from v5
//! (`src/graph/typegraph/rust/mod.rs`). Span bridge: syn's proc_macro2 spans are
//! line/col; v6 `Span` is byte offsets, so one `line_starts` table +
//! `line_col_to_byte` converts (the rust-specific bit oxc gives for free).
//!
//! Deferred to `Resolve<TypeF>` (commit 4): type EDGES (field/impl/variant/uses/
//! generic). Deferred follow-ups: the docs facet (`rust_docs_from`); the df
//! enrichment aux (args/fields/lits/param_pos/loops/nests).

use crate::family::CstF;
use crate::rows::FamilyBundle;
use crate::seams::{Parser, Project};
use crate::shape::Strings;
use crate::source::{ExtractOutput, FamilyMask, Source};
use super::astgrep::{AstGrepParser, CstProjector};

/// The Rust `Source`. `matches` = the path ends in `.rs`. cst via ast-grep's rust
/// grammar; type/call/df/const via one `syn::parse_file` (commits B/C/D).
#[derive(Default)]
pub struct RustSource;

impl Source for RustSource {
    fn name(&self) -> &'static str {
        "rust"
    }

    fn matches(&self, path: &str) -> bool {
        path.ends_with(".rs")
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();

        // cst via ast-grep (masked). ast-grep's SupportLang has a rust grammar, so
        // a .rs parses losslessly. Owns its () arena; dropped at block end. A failed
        // ast-grep parse leaves cst None (no panic).
        let cst = if mask.cst {
            let arena = AstGrepParser.make_arena();
            AstGrepParser.parse(&arena, path, content).ok().map(|parsed| {
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                bundle
            })
        } else {
            None
        };

        // type/call/df via ONE syn parse (masked). Commits B/C/D fill the
        // projections; this commit plumbs the parse only. A failed parse leaves all
        // three None (partial output: cst above may still be Some).
        if mask.types || mask.call || mask.df {
            if let Ok(src) = std::str::from_utf8(content) {
                if let Ok(_parsed) = syn::parse_file(src) {
                    // commit B: TypeF + const; C: CallF; D: DfF.
                }
            }
        }

        ExtractOutput { strings, cst, types: None, call: None, df: None }
    }
}
