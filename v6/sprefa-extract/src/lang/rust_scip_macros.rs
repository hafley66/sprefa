//! The scip macro post-pass: call edges for calls written inside macro
//! invocations.
//!
//! A call inside a macro invocation (`twice!(helper())`) is a token tree to
//! the syn parse, so the parse arm mints no call site and `Resolve<CallF>`
//! binds nothing for it. rust-analyzer's scip index still carries the exact
//! occurrence at the expanded position. This pass runs AFTER the per-file
//! resolve in `resolve_project`: for every rust file whose scip document
//! joined to its bytes, every reference occurrence that (a) sits inside a
//! macro invocation span, (b) is shaped like a call (identifier followed by
//! `(`), (c) matches no existing parse site, and (d) resolves through scip to
//! a corpus def, mints one `CallEdgeKind::ScipMacro` edge whose caller is the
//! innermost covering call def and whose call_site is the occurrence range.
//!
//! One shared `macro_site` row (`MacroSiteSource::Scip`) rides every minted
//! edge so the mbe lane's rows diff against it by span. Without a loaded scip
//! index the pass is a no-op and emits nothing: no index, no macro rows.

use std::collections::HashMap;

use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::lang::rust::{build_line_starts, syn_span};
use crate::scip::{byte_range_at, definition_of, LineTable};
use crate::seams::ProjectCx;
use crate::shape::{ContentId, Span};
use crate::source::ExtractOutput;
use crate::types::{
    containing_def_site, covering_def, CallEdgeKind, CallF, ProjectEdge, ResolutionOrigin,
};

/// One file handed to the post-pass. `project.rs` builds these in the same
/// order as the per-file resolved-edge list it already holds.
pub(crate) struct ScipMacroFile<'a> {
    pub path: &'a str,
    pub blob: &'a ContentId,
    pub output: &'a ExtractOutput,
}

/// One `macro_site` row: the invocation a minted edge came from, with the
/// index that bound it. `source` is `"scip"` for every row this pass emits;
/// the wire shape is the shared `FlatFact::MacroSiteOut` record.
#[derive(Clone, Debug)]
pub struct MacroSiteRow {
    pub path: String,
    pub span: Span,
    pub macro_name: String,
    pub source: &'static str,
}

/// The `macro_site` row's source tag for every row this pass emits.
pub const MACRO_SITE_SOURCE: &str = "scip";

/// A macro invocation seen in the parse: its byte span and the macro's name
/// (for a `macro_rules!` definition, the defined name inside the tokens).
struct InvocationSpan {
    span: Span,
    macro_name: String,
}

struct InvocationCollector {
    line_starts: Vec<u32>,
    invocations: Vec<InvocationSpan>,
}

impl<'ast> Visit<'ast> for InvocationCollector {
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        let span = syn_span(&self.line_starts, mac.span());
        let name = macro_name(mac);
        self.invocations.push(InvocationSpan {
            span,
            macro_name: name,
        });
        // The token trees stay opaque: descending would re-find the same
        // invocation's delimiters and nothing else, and a parse walk over
        // unexpanded tokens is exactly the gap this pass exists to close.
        syn::visit::visit_macro(self, mac);
    }
}

/// The macro's name as written. `macro_rules!` definitions parse with the
/// path `macro_rules` and the defined name as the first token; every other
/// invocation names itself in the path's trailing segment.
fn macro_name(mac: &syn::Macro) -> String {
    let trailing = mac
        .path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
        .unwrap_or_default();
    if trailing != "macro_rules" {
        return trailing;
    }
    mac.tokens
        .clone()
        .into_iter()
        .find_map(|token| match token {
            proc_macro2::TokenTree::Ident(ident) => Some(ident.to_string()),
            _ => None,
        })
        .unwrap_or(trailing)
}

/// Every macro invocation span in one file's bytes, smallest-last for the
/// innermost-containment pick. A file syn cannot parse yields none: the pass
/// mints nothing for it rather than guessing spans from text.
fn invocation_spans(content: &[u8]) -> Vec<InvocationSpan> {
    let Ok(text) = std::str::from_utf8(content) else {
        return Vec::new();
    };
    let Ok(file) = syn::parse_file(text) else {
        return Vec::new();
    };
    let mut collector = InvocationCollector {
        line_starts: build_line_starts(text),
        invocations: Vec::new(),
    };
    collector.visit_file(&file);
    collector
        .invocations
        .sort_by_key(|invocation| invocation.span.end() - invocation.span.start);
    collector.invocations
}

/// The identifier-shaped text at `span`, else None. A reference occurrence on
/// something that is not a plain identifier (a path qualifier, a string) is
/// never a call callee.
fn identifier_at(content: &[u8], span: Span) -> Option<String> {
    let text = content.get(span.start as usize..span.end() as usize)?;
    let text = std::str::from_utf8(text).ok()?;
    let mut chars = text.chars();
    let first = chars.next()?;
    if !first.is_alphabetic() && first != '_' {
        return None;
    }
    if !chars.all(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    Some(text.to_string())
}

/// Whether the first non-whitespace byte after `span` opens an argument list:
/// the call shape this pass mints for. A reference not followed by `(` is a
/// value or path mention, which the value-ref legs own.
fn followed_by_paren(content: &[u8], span: Span) -> bool {
    let rest = content.get(span.end() as usize..).unwrap_or(&[]);
    rest.iter()
        .find(|byte| !byte.is_ascii_whitespace())
        .is_some_and(|byte| *byte == b'(')
}

/// Whether some parse site already covers `span`: the call was written
/// outside the macro's opaque region (or the parse did see it), and minting
/// again would duplicate the edge.
fn covered_by_site(sites: &[(u32, u32)], span: Span) -> bool {
    let cut = sites.partition_point(|&(start, _)| start <= span.start);
    sites[..cut]
        .iter()
        .any(|&(start, end)| start <= span.start && span.end() <= end)
}

/// The post-pass entry. Extends `resolved` (parallel to `files`) with the
/// minted edges and returns the `macro_site` rows in file order. One document
/// join per file: the occurrence walk reads the already-joined bytes.
pub(crate) fn mint_macro_edges(
    files: &[ScipMacroFile<'_>],
    cx: &ProjectCx<'_>,
    resolved: &mut [(ContentId, Vec<ProjectEdge<CallF>>)],
) -> Vec<Vec<MacroSiteRow>> {
    let (Some(index), Some(reader)) = (cx.indexes.scip_index.get(), cx.reader) else {
        return Vec::new();
    };
    let Some(def_index) = cx.indexes.def_index.get() else {
        return Vec::new();
    };
    let joined = cx
        .indexes
        .joined_documents
        .get_or_init(|| crate::scip::join_documents(index, reader));
    let mut def_lines: HashMap<usize, LineTable> = HashMap::new();
    let mut rows: Vec<Vec<MacroSiteRow>> = Vec::new();
    for (file, (_blob, edges)) in files.iter().zip(resolved.iter_mut()) {
        rows.push(Vec::new());
        let Some(call) = &file.output.call else {
            continue;
        };
        // ONE document join per file: the slot whose content id equals the
        // file's blob. A document the reader cannot read is outside the
        // corpus, and nothing mints for it.
        let Some(doc_ix) = joined
            .iter()
            .position(|entry| entry.as_ref().map_or(false, |(b, _)| b == file.blob))
        else {
            continue;
        };
        let Some((_, content)) = joined[doc_ix].as_ref() else {
            continue;
        };
        let invocations = invocation_spans(content);
        if invocations.is_empty() {
            continue;
        }
        let lines = LineTable::build(content);
        let doc = &index.documents[doc_ix];
        let mut sites: Vec<(u32, u32)> = call
            .aux
            .sites
            .iter()
            .map(|site| (site.span.start, site.span.end()))
            .collect();
        sites.sort_unstable();
        for occ in &doc.occurrences {
            if occ.roles.contains(crate::types::OccurrenceRole::DEFINITION) {
                continue;
            }
            if index.symbol(occ.symbol).starts_with("local ") {
                continue;
            }
            let Some(span) = byte_range_at(content, &lines, occ.range, doc.position_encoding)
            else {
                continue;
            };
            if identifier_at(content, span).is_none() || !followed_by_paren(content, span) {
                continue;
            }
            if covered_by_site(&sites, span) {
                continue;
            }
            // The innermost invocation covering the occurrence names the row.
            let Some(invocation) = invocations.iter().find(|invocation| {
                invocation.span.start <= span.start && span.end() <= invocation.span.end()
            }) else {
                continue;
            };
            // scip's word on the target: symbol -> its definition occurrence
            // -> the corpus def containing it. A symbol with no in-corpus
            // definition (external, generated) mints nothing.
            let Some((def_doc_ix, def_range)) = definition_of(index, doc_ix, occ.symbol) else {
                continue;
            };
            let Some((def_blob, def_content)) = joined[def_doc_ix].as_ref() else {
                continue;
            };
            let def_doc = &index.documents[def_doc_ix];
            let def_table = def_lines
                .entry(def_doc_ix)
                .or_insert_with(|| LineTable::build(def_content));
            let Some(ident) =
                byte_range_at(def_content, def_table, def_range, def_doc.position_encoding)
            else {
                continue;
            };
            let Some((_, def_site)) = containing_def_site(def_index, def_blob.clone(), ident)
            else {
                continue;
            };
            let Some(caller) = covering_def(call, span) else {
                continue;
            };
            edges.push(
                ProjectEdge::new(
                    caller,
                    def_site.blob.clone(),
                    def_site.span,
                    CallEdgeKind::ScipMacro,
                    ResolutionOrigin::Scip,
                )
                .with_call_site(span),
            );
            rows.last_mut()
                .expect("one row slot per file, pushed above")
                .push(MacroSiteRow {
                    path: file.path.to_string(),
                    span: invocation.span,
                    macro_name: invocation.macro_name.clone(),
                    source: MACRO_SITE_SOURCE,
                });
        }
    }
    rows
}
