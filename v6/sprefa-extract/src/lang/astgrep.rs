//! The ast-grep Parser + the CstF projection (commit 1's only family).
//!
//! ast-grep-language ships the tree-sitter grammars for rust/ts/tsx/js/go in
//! ONE dep; `SupportLang::from_path` picks the grammar from the path. The parse
//! goes through `AstGrep::new` (the library, never a subprocess), so this abides
//! the trait seams. The CstF walk stays inside ast-grep's `Node` API
//! (`is_named` / `kind` / `range` / `children`): a port of v5
//! `src/cst.rs::walk_cst`, iterative pre-order DFS, named nodes only, unnamed
//! nodes reparenting their named descendants to the nearest named ancestor.

use crate::lang::extract_lang::ExtractLang;
use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::Language as _;
use ast_grep_core::{AstGrep, Node as SgNode, Pattern};
use ast_grep_language::SupportLang;
use serde::Serialize;

use crate::family::{CstEdgeKind, CstF};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{ParseError, Parser, Project};
use crate::shape::{NodeRef, Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;

/// The owned ast-grep root: owns its source `String` + the tree-sitter `Tree`.
/// `Send`; the borrowed `Node<'r>` is not, so projection (which walks it) runs on
/// the thread that owns the root. (v5 `src/sg.rs`: `type SgRoot =
/// AstGrep<StrDoc<ExtractLang>>`.)
pub type SgRoot = AstGrep<StrDoc<ExtractLang>>;

/// One generic ast-grep pattern and the single-node captures the caller wants
/// flattened. Query identity belongs to the caller's program; the extractor
/// only parses, matches, and returns byte-addressed facts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AstPatternQuery {
    pub id: String,
    pub pattern: String,
    pub selector: Option<String>,
    pub captures: Vec<String>,
}

/// One capture from one pattern match. Every field is top-level so JSONL host
/// projections remain flat. The whole-match span lets programs group captures
/// from the same match and join nested matches by containment.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AstCaptureFact {
    pub record: &'static str,
    pub query: String,
    pub capture: String,
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub match_start: u32,
    pub match_end: u32,
}

/// Run several patterns over one parsed source root and flatten only the
/// requested single-node captures. Pattern matching stays inside the extractor;
/// framework meaning is derived by ordinary relations over these rows.
pub fn query_patterns(
    path: &str,
    content: &[u8],
    queries: &[AstPatternQuery],
) -> Result<Vec<AstCaptureFact>, ParseError> {
    let lang =
        ExtractLang::from_path(path).ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
    let source =
        std::str::from_utf8(content).map_err(|error| ParseError::Utf8(error.to_string()))?;
    let root = AstGrep::new(source, lang);
    let mut facts = Vec::new();

    for query in queries {
        let pattern = match &query.selector {
            Some(selector) => Pattern::contextual(&query.pattern, selector, lang),
            None => Pattern::try_new(&query.pattern, lang),
        }
        .map_err(|error| {
            ParseError::Parse(format!("ast pattern '{}' failed: {error}", query.id))
        })?;
        let defined = pattern.defined_vars();
        for capture in &query.captures {
            if !defined.contains(capture.as_str()) {
                return Err(ParseError::Parse(format!(
                    "ast pattern '{}' does not define capture '{capture}'",
                    query.id
                )));
            }
        }
        for matched in root.root().find_all(&pattern) {
            let match_range = matched.range();
            for capture in &query.captures {
                let Some(node) = matched.get_env().get_match(capture) else {
                    continue;
                };
                let range = node.range();
                facts.push(AstCaptureFact {
                    record: "capture",
                    query: query.id.clone(),
                    capture: capture.clone(),
                    text: node.text().to_string(),
                    start: range.start as u32,
                    end: range.end as u32,
                    match_start: match_range.start as u32,
                    match_end: match_range.end as u32,
                });
            }
        }
    }

    facts.sort_by(|left, right| {
        (
            &left.query,
            left.match_start,
            left.match_end,
            &left.capture,
            left.start,
            left.end,
            &left.text,
        )
            .cmp(&(
                &right.query,
                right.match_start,
                right.match_end,
                &right.capture,
                right.start,
                right.end,
                &right.text,
            ))
    });
    facts.dedup();
    Ok(facts)
}

/// The Parser: one dep covers rust/ts/tsx/js/go via ast-grep's grammars.
#[derive(Default)]
pub struct AstGrepParser;

impl Parser for AstGrepParser {
    type Arena = ();
    type Parsed<'a> = SgRoot;

    fn name(&self) -> &'static str {
        "ast-grep"
    }

    fn matches(&self, path: &str) -> bool {
        // SupportLang directly: the roster's per-grammar Sources answer ahead of
        // this fallback, and ExtractLang::from_path routes through the roster,
        // so asking it here would recurse.
        SupportLang::from_path(path).is_some()
    }

    fn make_arena(&self) {}

    fn parse<'a>(
        &self,
        _arena: &'a (),
        path: &str,
        content: &'a [u8],
    ) -> Result<SgRoot, ParseError> {
        let lang =
            ExtractLang::from_path(path).ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
        let src = std::str::from_utf8(content).map_err(|err| ParseError::Utf8(err.to_string()))?;
        Ok(AstGrep::new(src, lang))
    }
}

/// The CstF projector: walks the parsed ast-grep tree, emitting one row per
/// named node + a `Child` edge to its nearest named ancestor.
#[derive(Default)]
pub struct CstProjector;

impl Project<CstF> for CstProjector {
    type Parsed<'a> = SgRoot;

    fn project(&self, root: &SgRoot, strings: &mut Strings, sink: &mut FamilyBundle<CstF>) {
        // Iterative pre-order DFS. Stack entries carry the node + the index of
        // its nearest named ancestor (None at the root). Unnamed punctuation
        // nodes emit no row but pass `nearest_named` through so their named
        // descendants attach to the nearest named ancestor. Children are pushed
        // in reverse so they pop in source order. (Port of v5 walk_cst.)
        let mut stack: Vec<(SgNode<StrDoc<ExtractLang>>, Option<NodeRef>)> =
            vec![(root.root(), None)];
        while let Some((node, nearest_named)) = stack.pop() {
            let my_named = if node.is_named() {
                let byte_range = node.range();
                let span = Span {
                    start: byte_range.start as u32,
                    len: (byte_range.end - byte_range.start) as u32,
                };
                let ix = NodeRef(sink.nodes.len() as u32);
                let kind = strings.intern(&*node.kind());
                sink.nodes.push(Node::new(span, kind));
                if let Some(parent_ix) = nearest_named {
                    // child edge: parent -> child (v5 `child(parent, child)`).
                    sink.edges
                        .push(Edge::new(parent_ix, ix, CstEdgeKind::Child));
                }
                Some(ix)
            } else {
                nearest_named
            };
            // Pushed forward then reversed IN PLACE on the stack's own tail: a
            // per-node `children().collect()` is one heap allocation per node.
            let mark = stack.len();
            for child in node.children() {
                stack.push((child, my_named));
            }
            stack[mark..].reverse();
        }
    }
}

// ════════════════════════════════════════════════════════════════════════════
// AstgrepSource: the CST-only Source (the floor for every ast-grep lang). Epic U.
//
// Commit 1's AstGrepParser + CstProjector repurposed as a Source: cst-only (the
// other families are None). The roster's fallback behind the lang-specific
// TsSource; a .rs/.go/... that has no native parser still gets the lossless CST.
// ════════════════════════════════════════════════════════════════════════════

/// The CST-only `Source`: every ast-grep grammar (rust/ts/tsx/js/go/...) gets the
/// lossless named-node tree, nothing else.
#[derive(Default)]
pub struct AstgrepSource;

impl Source for AstgrepSource {
    fn name(&self) -> &'static str {
        "astgrep"
    }

    fn matches(&self, path: &str) -> bool {
        AstGrepParser.matches(path)
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut strings = Strings::new();
        let cst = if mask.cst {
            let arena = AstGrepParser.make_arena();
            let parsed = {
                let span = trace::parse_span("astgrep", "astgrep");
                let _entered = span.enter();
                AstGrepParser.parse(&arena, path, content).ok()
            };
            parsed.map(|parsed| {
                let span = trace::family_span("astgrep", "cst");
                let _entered = span.enter();
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                trace::record_bundle(&span, &bundle, 0);
                bundle
            })
        } else {
            None
        };
        ExtractOutput {
            strings,
            cst,
            types: None,
            call: None,
            df: None,
            data: None,
        }
    }
}
