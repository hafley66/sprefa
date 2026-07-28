//! The ast-grep Parser + the CstF projection (commit 1's only family).
//!
//! ast-grep-language ships the tree-sitter grammars for rust/ts/tsx/js/go in
//! ONE dep; `SupportLang::from_path` picks the grammar from the path. The parse
//! goes through `AstGrep::new` (the library, never a subprocess), so this abides
//! the trait seams. The CstF walk stays inside ast-grep's `Node` API
//! (`is_named` / `kind` / `range` / `children`): a port of v5
//! `src/cst.rs::walk_cst`, iterative pre-order DFS, named nodes only, unnamed
//! nodes reparenting their named descendants to the nearest named ancestor.

use ast_grep_core::tree_sitter::StrDoc;
use ast_grep_core::{AstGrep, Language, Node as SgNode};
use ast_grep_language::SupportLang;

use crate::family::{CstEdgeKind, CstF};
use crate::rows::{Edge, FamilyBundle, Node};
use crate::seams::{ParseError, Parser, Project};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::shape::{NodeRef, Span, Strings};

/// The owned ast-grep root: owns its source `String` + the tree-sitter `Tree`.
/// `Send`; the borrowed `Node<'r>` is not, so projection (which walks it) runs on
/// the thread that owns the root. (v5 `src/sg.rs`: `type SgRoot =
/// AstGrep<StrDoc<SupportLang>>`.)
pub type SgRoot = AstGrep<StrDoc<SupportLang>>;

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
        SupportLang::from_path(path).is_some()
    }

    fn make_arena(&self) {}

    fn parse<'a>(&self, _arena: &'a (), path: &str, content: &'a [u8]) -> Result<SgRoot, ParseError> {
        let lang = SupportLang::from_path(path)
            .ok_or_else(|| ParseError::NoGrammar(path.to_string()))?;
        let src =
            std::str::from_utf8(content).map_err(|err| ParseError::Utf8(err.to_string()))?;
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
        let mut stack: Vec<(SgNode<StrDoc<SupportLang>>, Option<NodeRef>)> =
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
                    sink.edges.push(Edge::new(parent_ix, ix, CstEdgeKind::Child));
                }
                Some(ix)
            } else {
                nearest_named
            };
            let mut children: Vec<_> = node.children().collect();
            children.reverse();
            for child in children {
                stack.push((child, my_named));
            }
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
            AstGrepParser.parse(&arena, path, content).ok().map(|parsed| {
                let mut bundle = FamilyBundle::<CstF>::default();
                CstProjector.project(&parsed, &mut strings, &mut bundle);
                bundle
            })
        } else {
            None
        };
        ExtractOutput { strings, cst, types: None, call: None, df: None }
    }
}
