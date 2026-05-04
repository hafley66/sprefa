//! `re` — regex pattern DSL. TS-backed: a tiny grammar.js distinguishes
//! `$NAME` sugar from literal regex bytes. Compile rewrites each `$NAME` to
//! `(?P<NAME>.*?)` and hands the desugared string to `regex::bytes::Regex`.
//!
//! Sprf-blind. The v3 carveout for `${X?}` term-mode is omitted; lift it via
//! a wrapper Dsl in the consuming crate if needed.

use std::ops::ControlFlow;
use std::sync::{Arc, OnceLock};

use regex::bytes::Regex;
use tree_sitter::{Language, Parser as TSParser, Query, Tree};

use crate::cst::diag::{Diag, DiagSink};
use crate::cst::dsl::{CaptureKind, CaptureRow, CaptureSink, Compiled, Dsl};
use crate::cst::lsp::highlights::{
    default_capture_table, highlights_to_semantic_tokens, CaptureTable, Legend,
};
use crate::cst::lsp::providers::{DslBodyLsp, SemanticToken};
use crate::cst::path::{ts_default_emit, PathBuilder};

extern "C" {
    fn tree_sitter_sprefa_re() -> Language;
}

const RE_UNBOUND_HOLE: &str = ".*?";

pub struct ReDsl {
    legend:           Legend,
    table:            CaptureTable,
    highlights_query: OnceLock<Query>,
}

impl Default for ReDsl {
    fn default() -> Self { Self::new() }
}

impl ReDsl {
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

    pub fn language(&self) -> Language { unsafe { tree_sitter_sprefa_re() } }

    fn highlights_query(&self) -> &Query {
        self.highlights_query.get_or_init(|| {
            Query::new(
                &self.language(),
                include_str!("queries/highlights.scm"),
            )
            .expect("re/queries/highlights.scm")
        })
    }
}

impl Dsl for ReDsl {
    fn id(&self) -> &'static str { "re" }

    fn compile(
        &self,
        body:  &[u8],
        diags: &dyn DiagSink,
    ) -> Result<Box<dyn Compiled>, Diag> {
        let body_str = std::str::from_utf8(body).map_err(|e| {
            Diag::error("re.utf8", format!("body not utf-8: {e}"), 0..body.len())
        })?;
        let mut p = TSParser::new();
        p.set_language(&self.language()).map_err(|e| {
            Diag::error("re.language", format!("set_language failed: {e}"), 0..0)
        })?;
        let tree = p.parse(body_str, None).ok_or_else(|| {
            Diag::error("re.parse", "parser returned None", 0..0)
        })?;

        let (parts, sugar_names) = build_parts(&tree, body);
        let mut rx = String::new();
        for part in &parts {
            match part {
                Part::Lit(s)  => rx.push_str(s),
                Part::Term(n) => {
                    rx.push_str("(?P<");
                    rx.push_str(n);
                    rx.push('>');
                    rx.push_str(RE_UNBOUND_HOLE);
                    rx.push(')');
                }
            }
        }

        let regex = Regex::new(&rx).map_err(|err| {
            Diag::error("re.syntax", err.to_string(), 0..body.len())
        })?;

        // declared = sugar names first, then native named groups (deduped).
        let mut declared: Vec<Arc<str>> = sugar_names;
        for name in regex.capture_names().flatten() {
            let n: Arc<str> = Arc::from(name);
            if !declared.iter().any(|x| **x == *n) {
                declared.push(n);
            }
        }

        // No non-fatal diagnostics today; sink kept in the signature so it
        // stays available when we add e.g. "duplicate name" warnings later.
        let _ = diags;

        Ok(Box::new(ReCompiled { regex, tree, declared }))
    }

    fn injection_grammar(&self) -> Option<Language> { Some(self.language()) }

    fn lsp(&self) -> Option<&dyn DslBodyLsp> { Some(self) }
}

pub struct ReCompiled {
    regex:    Regex,
    tree:     Tree,
    declared: Vec<Arc<str>>,
}

impl ReCompiled {
    pub fn regex(&self) -> &Regex { &self.regex }
    pub fn tree(&self) -> &Tree { &self.tree }
}

impl Compiled for ReCompiled {
    fn declared_captures(&self) -> &[Arc<str>] { &self.declared }

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

impl DslBodyLsp for ReDsl {
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

enum Part { Lit(String), Term(Arc<str>) }

fn build_parts(tree: &Tree, bytes: &[u8]) -> (Vec<Part>, Vec<Arc<str>>) {
    let root = tree.root_node();
    let mut parts: Vec<Part>     = Vec::new();
    let mut sugar: Vec<Arc<str>> = Vec::new();
    let mut walker = root.walk();
    for child in root.named_children(&mut walker) {
        match child.kind() {
            "term_ref" => {
                let raw = std::str::from_utf8(&bytes[child.byte_range()]).unwrap_or("");
                let name = raw.trim_start_matches('$').trim_end_matches('?');
                let n: Arc<str> = Arc::from(name);
                parts.push(Part::Term(n.clone()));
                sugar.push(n);
            }
            "literal" => {
                if let Ok(s) = std::str::from_utf8(&bytes[child.byte_range()]) {
                    parts.push(Part::Lit(s.to_string()));
                }
            }
            _ => {}
        }
    }
    (parts, sugar)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cst::diag::SilentSink;
    use crate::cst::dsl::VecCaptureSink;

    #[test]
    fn compiles_plain_literal() {
        let dsl = ReDsl::new();
        let c = dsl.compile(br"TODO\(.+\)", &SilentSink).unwrap();
        assert!(c.declared_captures().is_empty());
    }

    #[test]
    fn sugar_lowers_to_named_group_and_declares_capture() {
        let dsl = ReDsl::new();
        let c = dsl.compile(br"TODO\($WHO\)", &SilentSink).unwrap();
        let names: Vec<&str> = c.declared_captures().iter().map(|a| &**a).collect();
        assert_eq!(names, vec!["WHO"]);

        let mut sink = VecCaptureSink::new();
        c.match_into(b"prefix TODO(alice) suffix", 100, &mut sink);
        assert_eq!(sink.rows.len(), 1);
        assert_eq!(&*sink.rows[0].name, "WHO");
        match &sink.rows[0].kind {
            CaptureKind::Span { byte_range } => {
                // "alice" starts at byte 12 of target → 100 + 12 = 112.
                assert_eq!(byte_range.start, 112);
                assert_eq!(byte_range.end,   117);
            }
            _ => panic!("expected Span"),
        }
    }

    #[test]
    fn sugar_and_native_named_group_dedup() {
        let dsl = ReDsl::new();
        let c = dsl
            .compile(br"$FIRST (?P<SECOND>\w+)", &SilentSink)
            .unwrap();
        let names: Vec<&str> =
            c.declared_captures().iter().map(|a| &**a).collect();
        assert!(names.contains(&"FIRST"));
        assert!(names.contains(&"SECOND"));
    }

    #[test]
    fn bad_regex_returns_fatal_diag() {
        let dsl = ReDsl::new();
        let err = match dsl.compile(b"[unclosed", &SilentSink) {
            Err(d)  => d,
            Ok(_)   => panic!("expected compile failure"),
        };
        assert_eq!(err.code, "re.syntax");
    }

    #[test]
    fn semantic_tokens_emit_for_term_ref_and_literal() {
        let dsl = ReDsl::new();
        let toks = dsl.semantic_tokens(br"TODO\($WHO\)");
        // Expect at least two tokens: literal + term_ref.
        assert!(toks.len() >= 2, "got {} tokens: {:?}", toks.len(), toks);
        let function_idx = dsl.legend.type_index(&lsp_types::SemanticTokenType::FUNCTION).unwrap();
        let _ = function_idx;
    }
}
