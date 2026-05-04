//! `glob` — path-glob pattern DSL. TS-backed: a tiny grammar.js distinguishes
//! `*`, `**`, `?`, `/`, `{a,b}`, `$NAME`, and literal byte runs. Compile lowers
//! the segment stream into an anchored byte-regex (`^…$`).
//!
//! Sprf-blind. The v3 carveouts (TermMode, materialize_template, anchors
//! plumbing for repo/rev/fs reuse) are dropped; lift them via a wrapper Dsl
//! in the consuming crate if needed.

use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock};

use regex::bytes::Regex;
use tree_sitter::{Language, Node, Parser as TSParser, Query, Tree};

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureKind, CaptureRow, CaptureSink, Compiled, Dsl};
use crate::cst::lsp::highlights::{
    default_capture_table, highlights_to_semantic_tokens, CaptureTable, Legend,
};
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};
use crate::cst::path::{ts_default_emit, PathBuilder};

extern "C" {
    fn tree_sitter_sprefa_glob() -> Language;
}

/// Per-DSL default regex fragment for an unbound `$NAME` hole.
/// Glob holes never cross a `/` boundary.
const GLOB_UNBOUND_HOLE: &str = "[^/]+";

pub struct GlobDsl {
    legend:           Legend,
    table:            CaptureTable,
    highlights_query: OnceLock<Query>,
}

impl Default for GlobDsl {
    fn default() -> Self { Self::new() }
}

impl GlobDsl {
    pub fn new() -> Self {
        Self {
            legend:           Legend::standard(),
            table:            default_capture_table(),
            highlights_query: OnceLock::new(),
        }
    }

    pub fn with_legend(legend: Legend) -> Self {
        Self {
            legend,
            table:            default_capture_table(),
            highlights_query: OnceLock::new(),
        }
    }

    pub fn language(&self) -> Language { unsafe { tree_sitter_sprefa_glob() } }

    fn highlights_query(&self) -> &Query {
        self.highlights_query.get_or_init(|| {
            Query::new(
                &self.language(),
                include_str!("queries/highlights.scm"),
            )
            .expect("glob/queries/highlights.scm")
        })
    }
}

impl Dsl for GlobDsl {
    fn id(&self) -> &'static str { "glob" }

    fn compile(
        &self,
        body:  &[u8],
        diags: &dyn DiagSink,
    ) -> Result<Box<dyn Compiled>, Diag> {
        let body_str = std::str::from_utf8(body).map_err(|e| {
            Diag::error("glob.utf8", format!("body not utf-8: {e}"), 0..body.len())
        })?;
        let mut p = TSParser::new();
        p.set_language(&self.language()).map_err(|e| {
            Diag::error("glob.language", format!("set_language failed: {e}"), 0..0)
        })?;
        let tree = p.parse(body_str, None).ok_or_else(|| {
            Diag::error("glob.parse", "parser returned None", 0..0)
        })?;

        let (parts, _hole_names) = build_parts(&tree, body);

        let mut rx = String::from("^");
        for part in &parts {
            match part {
                Part::Frag(s) => rx.push_str(s),
                Part::Term(n) => {
                    rx.push_str("(?P<");
                    rx.push_str(n);
                    rx.push('>');
                    rx.push_str(GLOB_UNBOUND_HOLE);
                    rx.push(')');
                }
            }
        }
        rx.push('$');

        let regex = Regex::new(&rx).map_err(|err| {
            Diag::error("glob.syntax", err.to_string(), 0..body.len())
        })?;

        let _ = diags;

        Ok(Box::new(GlobCompiled { regex, tree }))
    }

    fn injection_grammar(&self) -> Option<Language> { Some(self.language()) }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> { Some(self) }
}

pub struct GlobCompiled {
    regex: Regex,
    tree:  Tree,
}

impl GlobCompiled {
    pub fn regex(&self) -> &Regex { &self.regex }
    pub fn tree(&self)  -> &Tree  { &self.tree }
}

impl Compiled for GlobCompiled {
    fn match_into(&self, target: &[u8], target_off: usize, sink: &mut dyn CaptureSink) {
        let names: Vec<Option<&str>> = self.regex.capture_names().collect();
        for caps in self.regex.captures_iter(target) {
            for (i, name_opt) in names.iter().enumerate() {
                let Some(name) = name_opt else { continue };
                let Some(m) = caps.get(i) else { continue };
                let row = CaptureRow {
                    name: Arc::from(*name),
                    kind: CaptureKind::Span {
                        byte_range: (target_off + m.start())..(target_off + m.end()),
                    },
                };
                if let ControlFlow::Break(_) = sink.emit(row) { return; }
            }
        }
    }

    fn emit_path_items(&self, body_byte: usize, builder: &mut PathBuilder) {
        ts_default_emit(&self.tree, body_byte, builder);
    }
}

impl DslBodyLsp for GlobDsl {
    fn semantic_tokens(&self, body: &[u8]) -> Vec<SemanticToken> {
        let mut p = TSParser::new();
        if p.set_language(&self.language()).is_err() { return vec![]; }
        let body_str = match std::str::from_utf8(body) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let Some(tree) = p.parse(body_str, None) else { return vec![]; };
        highlights_to_semantic_tokens(
            self.highlights_query(),
            &tree,
            body,
            &self.legend,
            &self.table,
        )
    }
}

enum Part { Frag(String), Term(Arc<str>) }

fn build_parts(tree: &Tree, bytes: &[u8]) -> (Vec<Part>, Vec<Arc<str>>) {
    let root = tree.root_node();
    let mut parts: Vec<Part>     = Vec::new();
    let mut holes: Vec<Arc<str>> = Vec::new();

    let mut walker = root.walk();
    let kids: Vec<Node<'_>> = root.named_children(&mut walker).collect();
    let kind_at = |i: usize| kids.get(i).map(|n| n.kind()).unwrap_or("");

    let mut i = 0;
    while i < kids.len() {
        // `**` segment recognizers — match ripgrep/globset semantics.
        if kind_at(i) == "slash"
            && kind_at(i + 1) == "double_star"
            && kind_at(i + 2) == "slash"
        {
            parts.push(Part::Frag("(?:/[^/]+)*/".to_string()));
            i += 3;
            continue;
        }
        if kind_at(i) == "double_star" && kind_at(i + 1) == "slash" {
            parts.push(Part::Frag("(?:[^/]+/)*".to_string()));
            i += 2;
            continue;
        }
        if kind_at(i) == "slash" && kind_at(i + 1) == "double_star" {
            parts.push(Part::Frag("(?:/[^/]+)*".to_string()));
            i += 2;
            continue;
        }

        let child = kids[i];
        match child.kind() {
            "double_star" => parts.push(Part::Frag(".*".to_string())),
            "star"        => parts.push(Part::Frag("[^/]*".to_string())),
            "question"    => parts.push(Part::Frag("[^/]".to_string())),
            "slash"       => parts.push(Part::Frag("/".to_string())),
            "term_ref" => {
                let raw = std::str::from_utf8(&bytes[child.byte_range()]).unwrap_or("");
                let name = raw.trim_start_matches('$').trim_end_matches('?');
                let n: Arc<str> = Arc::from(name);
                parts.push(Part::Term(n.clone()));
                holes.push(n);
            }
            "literal" => {
                if let Ok(s) = std::str::from_utf8(&bytes[child.byte_range()]) {
                    parts.push(Part::Frag(regex::escape(s)));
                }
            }
            "brace_alt" => {
                let mut alt_walker = child.walk();
                let mut alts: Vec<String> = Vec::new();
                for item in child.named_children(&mut alt_walker) {
                    if item.kind() != "brace_alt_item" { continue; }
                    if let Ok(s) = std::str::from_utf8(&bytes[item.byte_range()]) {
                        alts.push(regex::escape(s));
                    }
                }
                if alts.is_empty() {
                    parts.push(Part::Frag(String::new()));
                } else {
                    parts.push(Part::Frag(format!("(?:{})", alts.join("|"))));
                }
            }
            _ => {}
        }
        i += 1;
    }

    (parts, holes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::diag::SilentSink;
    use crate::cst::dsl::VecCaptureSink;

    fn compile_concrete(src: &str) -> GlobCompiled {
        let dsl = GlobDsl::new();
        let mut p = TSParser::new();
        p.set_language(&dsl.language()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let (parts, _holes) = build_parts(&tree, src.as_bytes());
        let mut rx = String::from("^");
        for part in &parts {
            match part {
                Part::Frag(s) => rx.push_str(s),
                Part::Term(n) => {
                    rx.push_str("(?P<");
                    rx.push_str(n);
                    rx.push('>');
                    rx.push_str(GLOB_UNBOUND_HOLE);
                    rx.push(')');
                }
            }
        }
        rx.push('$');
        let regex = Regex::new(&rx).unwrap();
        GlobCompiled { regex, tree }
    }

    #[test]
    fn anchored_regex_for_literal() {
        let g = compile_concrete("foo.txt");
        assert_eq!(g.regex.as_str(), r"^foo\.txt$");
    }

    #[test]
    fn star_and_question_lower_to_non_slash_classes() {
        let g = compile_concrete("a*b?c");
        assert_eq!(g.regex.as_str(), r"^a[^/]*b[^/]c$");
    }

    #[test]
    fn hole_becomes_named_group_and_emits_capture() {
        let g = compile_concrete("**/$DIR/file.txt");
        assert!(g.regex.as_str().contains("(?P<DIR>[^/]+)"));

        let mut sink = VecCaptureSink::new();
        let c: Box<dyn Compiled> = Box::new(g);
        c.match_into(b"a/b/middle/file.txt", 50, &mut sink);
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(&*sink.rows[0].name, "DIR");
        match &sink.rows[0].kind {
            CaptureKind::Span { byte_range } => {
                // "middle" begins at byte 4 of target → 50 + 4 = 54.
                assert_eq!(byte_range.start, 54);
                assert_eq!(byte_range.end,   60);
            }
            _ => panic!("expected Span"),
        }
    }

    #[test]
    fn brace_alt_lowers_to_regex_alternation() {
        let g = compile_concrete("**/*.{ts,tsx}");
        assert!(g.regex.as_str().contains("(?:ts|tsx)"));
        assert!(g.regex.is_match(b"src/foo/bar.ts"));
        assert!(g.regex.is_match(b"src/foo/bar.tsx"));
        assert!(!g.regex.is_match(b"src/foo/bar.js"));
    }

    #[test]
    fn double_star_slash_matches_zero_or_more_segments() {
        let g = compile_concrete("src/**/*.rs");
        assert!(g.regex.is_match(b"src/foo.rs"));
        assert!(g.regex.is_match(b"src/a/foo.rs"));
        assert!(g.regex.is_match(b"src/a/b/foo.rs"));
        assert!(!g.regex.is_match(b"other/foo.rs"));
    }

    #[test]
    fn leading_double_star_slash_matches_zero_prefix() {
        let g = compile_concrete("**/*.rs");
        assert!(g.regex.is_match(b"foo.rs"));
        assert!(g.regex.is_match(b"a/foo.rs"));
        assert!(g.regex.is_match(b"a/b/foo.rs"));
        assert!(!g.regex.is_match(b"foo.md"));
    }

    #[test]
    fn trailing_slash_double_star_matches_zero_suffix() {
        let g = compile_concrete("src/**");
        assert!(g.regex.is_match(b"src/lib.rs"));
        assert!(g.regex.is_match(b"src/a/b.rs"));
        assert!(g.regex.is_match(b"src"));
    }

    #[test]
    fn bad_pattern_returns_fatal_diag() {
        // Glob bodies always parse (TS recovers); force a regex-syntax fail
        // by injecting an unmatched `(` via a literal that already escaped
        // can't happen — instead drive the failure by feeding bytes the
        // grammar rejects as non-utf8.
        let dsl = GlobDsl::new();
        let bad: &[u8] = &[0xff, 0xfe];
        let err = match dsl.compile(bad, &SilentSink) {
            Err(d) => d,
            Ok(_)  => panic!("expected compile failure"),
        };
        assert_eq!(err.code, "glob.utf8");
    }

    #[test]
    fn semantic_tokens_emit_for_literal_and_star() {
        let dsl = GlobDsl::new();
        let toks = dsl.semantic_tokens(b"src/**/*.rs");
        assert!(!toks.is_empty(), "expected highlight tokens, got {:?}", toks);
    }
}
