//! `re(...)` — regular expression pattern op (§14.2 / §14.7).
//!
//! Grammar distinguishes `term_ref` (`$NAME` sugar) from everything else
//! (literal regex bytes). [`ReOp::compile_from_tree`] rewrites each
//! `term_ref` into a non-greedy named group `(?P<NAME>.*?)` and hands
//! the concatenated string to `regex::bytes::Regex`. Compiled state
//! lives directly on the op instance (post-§14.5m fusion).
//!
//! `binds_captures` returns both the sugar names and the regex-native
//! named groups so downstream `> $NAME` resolution treats the two
//! origins identically (§14.7).
//!
//! At arg position the outer op reads [`Op::try_raw_regex`] and
//! [`Op::materialize_with`]. At pipe-step position `Op::pipe` runs
//! the shared [`apply_identity_pattern`] with find-all semantics.

use std::collections::HashMap;
use std::sync::Arc;

use regex::bytes::Regex;
use tree_sitter::{Node, Tree};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::pattern_op::{PatternDiagnostic, PatternOp};
use crate::value::{apply_identity_pattern, materialize_template, Seg};
use effect_runtime::{BoxFuture, RtCtx};

/// Per-op default regex fragment for an unbound `$NAME` hole. Non-
/// greedy so chained holes don't swallow each other.
const RE_UNBOUND_HOLE: &str = ".*?";

#[derive(Debug, Clone)]
pub struct ReOp {
    pub template: Vec<Seg>,
    pub regex: Regex,
    pub bound_captures: Vec<Arc<str>>,
    pub anchors: (Arc<str>, Arc<str>),
}

impl Default for ReOp {
    fn default() -> Self {
        ReOp {
            template: Vec::new(),
            regex: Regex::new(r"$^").expect("never-matching regex"),
            bound_captures: Vec::new(),
            anchors: (Arc::from(""), Arc::from("")),
        }
    }
}

impl ReOp {
    pub fn compile_from_tree(
        tree: &Tree,
        bytes: &[u8],
    ) -> Result<Self, Vec<PatternDiagnostic>> {
        let (template, sugar_names) = build_template(tree, bytes);

        let mut rx = String::new();
        for seg in &template {
            match seg {
                Seg::Fragment(s) => rx.push_str(s),
                Seg::Term { name, unbound_re, .. } => {
                    rx.push_str("(?P<");
                    rx.push_str(name);
                    rx.push('>');
                    rx.push_str(unbound_re);
                    rx.push(')');
                }
            }
        }

        let regex = Regex::new(&rx).map_err(|err| {
            vec![PatternDiagnostic {
                code: "pattern/re-syntax",
                message: err.to_string(),
                byte_range: 0..bytes.len(),
            }]
        })?;

        // bound_captures = sugar holes first, then native named groups
        // from the compiled regex (dedup against sugar).
        let mut bound = sugar_names;
        for name in regex.capture_names().flatten() {
            let n: Arc<str> = Arc::from(name);
            if !bound.iter().any(|existing| *existing == n) {
                bound.push(n);
            }
        }

        Ok(ReOp {
            template,
            regex,
            bound_captures: bound,
            anchors: (Arc::from(""), Arc::from("")),
        })
    }
}

impl crate::op_ctor::PatternCtor for ReOp {
    const NAME: &'static str = "re";
    const DOC:  &'static str = "\
**re**(_regex_)

Regex pattern op. `$NAME` sugar lowers to `(?P<NAME>.*?)`; native
`(?P<X>...)` named groups are also exposed via `bound_captures`.
At arg position, downstream consumers consume `try_raw_regex` for bulk
matching; at pipe-step position, runs find-all against `cursor.active()`.
";

    fn language() -> tree_sitter::Language {
        crate::op_languages::language_of("re").expect("re grammar registered")
    }

    fn highlights() -> Option<&'static str> {
        crate::op_languages::highlights_of("re")
    }

    fn compile_from_tree(tree: &Tree, bytes: &[u8]) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::compile_from_tree(tree, bytes)
    }
}

impl Op for ReOp {
    fn name(&self) -> &'static str { "re" }

    fn pipe<'a>(&'a self, _ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            apply_identity_pattern(
                &self.regex,
                &self.bound_captures,
                &self.template,
                (&self.anchors.0, &self.anchors.1),
                &c,
            )
        })
    }

    fn language(&self) -> Option<tree_sitter::Language> {
        crate::op_languages::language_of("re")
    }

    fn highlights(&self) -> Option<&'static str> {
        crate::op_languages::highlights_of("re")
    }

    fn bound_captures(&self) -> &[Arc<str>] {
        &self.bound_captures
    }

    fn try_raw_regex(&self) -> Option<&Regex> {
        if self.template.is_empty() {
            None
        } else {
            Some(&self.regex)
        }
    }

    fn materialize_with(
        &self,
        bindings: &HashMap<Arc<str>, Vec<u8>>,
    ) -> Option<Regex> {
        if self.template.is_empty() {
            return None;
        }
        materialize_template(
            &self.template,
            (&self.anchors.0, &self.anchors.1),
            bindings,
        )
        .ok()
    }
}

impl PatternOp for ReOp {
    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>> {
        let mut out = Vec::new();

        // Sugar first: $NAME nodes from the injected tree.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "term_ref" {
                out.push(term_ref_parts(child, bytes).0);
            }
        }

        // Native named groups from the desugared source (dedup against sugar).
        let desugared = desugar(tree, bytes);
        if let Ok(re) = Regex::new(&desugared) {
            for name in re.capture_names().flatten() {
                let n: Arc<str> = Arc::from(name);
                if !out.iter().any(|existing| *existing == n) {
                    out.push(n);
                }
            }
        }

        out
    }
}

fn build_template(tree: &Tree, bytes: &[u8]) -> (Vec<Seg>, Vec<Arc<str>>) {
    let root = tree.root_node();
    let mut template: Vec<Seg> = Vec::new();
    let mut sugar: Vec<Arc<str>> = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "term_ref" => {
                let (name, mode) = term_ref_parts(child, bytes);
                template.push(Seg::Term {
                    name: name.clone(),
                    unbound_re: Arc::from(RE_UNBOUND_HOLE),
                    mode,
                });
                sugar.push(name);
            }
            "literal" => {
                if let Ok(s) = std::str::from_utf8(&bytes[child.byte_range()]) {
                    template.push(Seg::Fragment(Arc::from(s)));
                }
            }
            _ => {}
        }
    }
    (template, sugar)
}

fn desugar(tree: &Tree, bytes: &[u8]) -> String {
    let (template, _) = build_template(tree, bytes);
    let mut out = String::new();
    for seg in &template {
        match seg {
            Seg::Fragment(s) => out.push_str(s),
            Seg::Term { name, unbound_re, .. } => {
                out.push_str("(?P<");
                out.push_str(name);
                out.push('>');
                out.push_str(unbound_re);
                out.push(')');
            }
        }
    }
    out
}

fn term_ref_parts(node: Node<'_>, bytes: &[u8]) -> (Arc<str>, crate::value::TermMode) {
    let raw = std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("");
    let stripped = raw.trim_start_matches('$');
    if let Some(name) = stripped.strip_suffix('?') {
        (Arc::from(name), crate::value::TermMode::Unbound)
    } else {
        (Arc::from(stripped), crate::value::TermMode::Read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::{Capture, Cursor};
    use std::sync::Arc;
    use tree_sitter::Parser;

    fn parse_re(src: &str) -> Tree {
        let mut p = Parser::new();
        let lang = crate::op_languages::language_of("re")
            .expect("re language registered");
        p.set_language(&lang).unwrap();
        p.parse(src, None).unwrap()
    }

    fn compile(src: &str) -> ReOp {
        let tree = parse_re(src);
        ReOp::compile_from_tree(&tree, src.as_bytes()).unwrap()
    }

    #[test]
    fn compile_shapes_and_binds_captures() {
        // Plain literal regex.
        let r = compile(r"TODO\(.+\)");
        assert!(r.regex.is_match(b"TODO(fix this)"));
        assert!(r.bound_captures.is_empty());

        // $NAME sugar.
        let r = compile(r"TODO\($WHO\)");
        assert!(r.regex.as_str().contains("(?P<WHO>.*?)"));
        let caps = r.regex.captures(b"TODO(alice)").unwrap();
        assert_eq!(caps.name("WHO").unwrap().as_bytes(), b"alice");
        assert_eq!(
            r.bound_captures.iter().map(|a| &**a).collect::<Vec<_>>(),
            vec!["WHO"]
        );

        // Sugar + native named group coexist.
        let tree = parse_re(r"$FIRST (?P<SECOND>\w+)");
        let names: Vec<String> = ReOp::default()
            .binds_captures(&tree, br"$FIRST (?P<SECOND>\w+)")
            .into_iter()
            .map(|a| a.to_string())
            .collect();
        assert!(names.contains(&"FIRST".to_string()));
        assert!(names.contains(&"SECOND".to_string()));
    }

    #[test]
    fn bad_regex_surfaces_diag() {
        let src = "[unclosed";
        let tree = parse_re(src);
        let e = ReOp::compile_from_tree(&tree, src.as_bytes()).unwrap_err();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].code, "pattern/re-syntax");
    }

    #[tokio::test]
    async fn apply_write_mode_and_read_mode() {
        let op: Arc<dyn Op> = Arc::new(compile(r"TODO\($WHO\)"));
        let ctx = RtCtx::default();

        // Write mode.
        let write_in = Cursor::new(Arc::from(b"TODO(alice) and TODO(bob)".as_slice()));
        let out = op.pipe(&ctx, write_in).await;
        assert_eq!(out.len(), 2);
        assert_eq!(&out[0].content[out[0].byte_range.clone()], b"TODO(alice)");
        let who0 = out[0].captures.iter().find(|c| &*c.name == "WHO").unwrap();
        assert_eq!(&out[0].content[who0.byte_range.clone()], b"alice");
        assert_eq!(&out[1].content[out[1].byte_range.clone()], b"TODO(bob)");

        // Read mode.
        let content: Arc<[u8]> = Arc::from(b"TODO(alice) and TODO(bob)".as_slice());
        let mut read_in = Cursor::new(content.clone());
        read_in.captures.push(Capture::span_backed(Arc::from("WHO"), 5..10));
        let out = op.pipe(&ctx, read_in).await;
        assert_eq!(out.len(), 1);
        assert_eq!(&out[0].content[out[0].byte_range.clone()], b"TODO(alice)");
        assert!(!out[0].captures.iter().any(|c| &*c.name == "WHO" && c.byte_range.start != 5));
    }
}
