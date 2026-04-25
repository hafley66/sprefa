//! `glob(...)` — path-glob pattern op (§14.2 / §14.7).
//!
//! Sub-grammar in `grammar.js` + committed `src/parser.c`.
//! [`GlobOp::compile_from_tree`] walks the injected tree and lowers
//! each segment into an anchored byte-regex. The compiled state lives
//! directly on the op instance (post-§14.5m fusion — no separate
//! `GlobPattern` wrapper).
//!
//! Segment lowering (ripgrep/globset-style `**`):
//!   `/**/`  → `(?:/[^/]+)*/`   (zero or more interior segments)
//!   `**/`   → `(?:[^/]+/)*`    (at start; zero or more leading segments)
//!   `/**`   → `(?:/[^/]+)*`    (at end; zero or more trailing segments)
//!   `**`    → `.*`             (bare, unusual — preserves old behavior)
//!   `*`     → `[^/]*`
//!   `?`     → `[^/]`
//!   `/`     → `/`
//!   `$NAME` → `(?P<NAME>[^/]+)`
//!   literal → `regex::escape(...)`
//!
//! The whole thing is bracketed with `^…$` so the compiled regex
//! matches a path string end-to-end.
//!
//! At arg position the outer op reads [`Op::try_raw_regex`] for
//! all-unbound fast-path and [`Op::materialize_with`] when at least
//! one term is bound upstream. At pipe-step position `Op::pipe` runs
//! the shared [`apply_identity_pattern`].

use std::collections::HashMap;
use std::sync::Arc;

use regex::bytes::Regex;
use tree_sitter::{Node, Tree};

use crate::_0_cursor::Cursor;
use crate::_1_op::Op;
use crate::pattern_op::{PatternDiagnostic, PatternOp};
use crate::value::{apply_identity_pattern, materialize_template, Seg};
use effect_runtime::{BoxFuture, RtCtx};

/// Per-op default regex fragment for an unbound `$NAME` hole.
/// Glob holes never cross a `/` boundary.
const GLOB_UNBOUND_HOLE: &str = "[^/]+";

#[derive(Debug, Clone)]
pub struct GlobOp {
    /// Ordered template; drives `materialize_with`.
    pub template: Vec<Seg>,
    /// All-unbound compiled regex, anchored end-to-end (`^…$`).
    pub regex: Regex,
    /// `$NAME` holes this pattern declares.
    pub bound_captures: Vec<Arc<str>>,
    /// Anchors baked into `regex`.
    pub anchors: (Arc<str>, Arc<str>),
}

impl Default for GlobOp {
    /// Empty, never-matching instance. Exists because the registry's
    /// `binds_captures` probe path needs a constructible handle before
    /// compilation.
    fn default() -> Self {
        GlobOp {
            template: Vec::new(),
            regex: Regex::new(r"$^").expect("never-matching regex"),
            bound_captures: Vec::new(),
            anchors: (Arc::from("^"), Arc::from("$")),
        }
    }
}

impl GlobOp {
    /// Compile an injected-tree body into a fully-formed `GlobOp`.
    pub fn compile_from_tree(
        tree: &Tree,
        bytes: &[u8],
    ) -> Result<Self, Vec<PatternDiagnostic>> {
        let root = tree.root_node();
        let mut template: Vec<Seg> = Vec::new();
        let mut holes: Vec<Arc<str>> = Vec::new();
        let mut walk_cursor = root.walk();
        let kids: Vec<Node<'_>> = root.named_children(&mut walk_cursor).collect();
        let kind_at = |i: usize| kids.get(i).map(|n| n.kind()).unwrap_or("");
        let mut i = 0;
        while i < kids.len() {
            if kind_at(i) == "slash"
                && kind_at(i + 1) == "double_star"
                && kind_at(i + 2) == "slash"
            {
                template.push(Seg::Fragment(Arc::from("(?:/[^/]+)*/")));
                i += 3;
                continue;
            }
            if kind_at(i) == "double_star" && kind_at(i + 1) == "slash" {
                template.push(Seg::Fragment(Arc::from("(?:[^/]+/)*")));
                i += 2;
                continue;
            }
            if kind_at(i) == "slash" && kind_at(i + 1) == "double_star" {
                template.push(Seg::Fragment(Arc::from("(?:/[^/]+)*")));
                i += 2;
                continue;
            }
            let child = kids[i];
            match child.kind() {
                "double_star" => template.push(Seg::Fragment(Arc::from(".*"))),
                "star" => template.push(Seg::Fragment(Arc::from("[^/]*"))),
                "question" => template.push(Seg::Fragment(Arc::from("[^/]"))),
                "slash" => template.push(Seg::Fragment(Arc::from("/"))),
                "term_ref" => {
                    let (name, mode) = term_ref_parts(child, bytes);
                    template.push(Seg::Term {
                        name: name.clone(),
                        unbound_re: Arc::from(GLOB_UNBOUND_HOLE),
                        mode,
                    });
                    holes.push(name);
                }
                "literal" => {
                    if let Ok(s) = std::str::from_utf8(&bytes[child.byte_range()]) {
                        template.push(Seg::Fragment(Arc::from(regex::escape(s))));
                    }
                }
                _ => {}
            }
            i += 1;
        }

        let mut rx = String::from("^");
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
        rx.push('$');

        let regex = Regex::new(&rx).map_err(|err| {
            vec![PatternDiagnostic {
                code: "pattern/glob-syntax",
                message: err.to_string(),
                byte_range: 0..bytes.len(),
            }]
        })?;
        Ok(GlobOp {
            template,
            regex,
            bound_captures: holes,
            anchors: (Arc::from("^"), Arc::from("$")),
        })
    }
}

impl crate::op_ctor::PatternCtor for GlobOp {
    const NAME: &'static str = "glob";
    const BODY_GRAMMAR: &'static str = "glob (`*` `**` `?` `$NAME`)";
    const DOC:  &'static str = "\
**glob**(_pattern_)

Glob pattern op (`**`, `*`, `?`, literals, `$NAME` holes). Compiles to
an anchored regex; `try_raw_regex` exposes it for bulk consumers
(`fs`, `repo`, `rev`, `comment`). At pipe-step position, runs as an
identity-projecting match against `cursor.active()`.
";

    fn language() -> tree_sitter::Language {
        crate::op_languages::language_of("glob").expect("glob grammar registered")
    }

    fn highlights() -> Option<&'static str> {
        crate::op_languages::highlights_of("glob")
    }

    fn compile_from_tree(tree: &Tree, bytes: &[u8]) -> Result<Self, Vec<PatternDiagnostic>> {
        Self::compile_from_tree(tree, bytes)
    }
}

impl Op for GlobOp {
    fn name(&self) -> &'static str { "glob" }

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
        crate::op_languages::language_of("glob")
    }

    fn highlights(&self) -> Option<&'static str> {
        crate::op_languages::highlights_of("glob")
    }

    fn bound_captures(&self) -> &[Arc<str>] {
        &self.bound_captures
    }

    fn try_raw_regex(&self) -> Option<&Regex> {
        // Empty template = default / uncompiled instance. Signal via
        // None so consumers know to skip bulk-dispatch.
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

impl PatternOp for GlobOp {
    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>> {
        let root = tree.root_node();
        let mut out = Vec::new();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "term_ref" {
                out.push(term_ref_parts(child, bytes).0);
            }
        }
        out
    }
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

/// Compile a raw glob source string without staging a tree-sitter parse.
/// Used by ops that reuse the glob surface (`repo`, `rev`, `fs`) and
/// test fixtures.
pub fn compile_str(src: &str) -> Result<GlobOp, Vec<PatternDiagnostic>> {
    use tree_sitter::Parser;
    let mut p = Parser::new();
    let lang = crate::op_languages::language_of("glob").ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "pattern/glob-language-missing",
            message: "glob sub-grammar not registered".into(),
            byte_range: 0..src.len(),
        }]
    })?;
    p.set_language(&lang).map_err(|e| {
        vec![PatternDiagnostic {
            code: "pattern/glob-language-set",
            message: e.to_string(),
            byte_range: 0..src.len(),
        }]
    })?;
    let tree = p.parse(src, None).ok_or_else(|| {
        vec![PatternDiagnostic {
            code: "pattern/glob-parse",
            message: "tree-sitter returned no tree".into(),
            byte_range: 0..src.len(),
        }]
    })?;
    GlobOp::compile_from_tree(&tree, src.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::_0_cursor::Cursor;
    use std::sync::Arc;
    use tree_sitter::Parser;

    fn parse_glob(src: &str) -> Tree {
        let mut p = Parser::new();
        let lang = crate::op_languages::language_of("glob")
            .expect("glob language registered");
        p.set_language(&lang).unwrap();
        p.parse(src, None).unwrap()
    }

    fn compile(src: &str) -> GlobOp {
        let tree = parse_glob(src);
        GlobOp::compile_from_tree(&tree, src.as_bytes()).unwrap()
    }

    #[test]
    fn plain_literal_compiles_to_escaped_anchored_regex() {
        let g = compile("foo.txt");
        assert_eq!(g.regex.as_str(), r"^foo\.txt$");
        assert!(g.bound_captures.is_empty());
    }

    #[test]
    fn hole_becomes_named_group() {
        let g = compile("**/$DIR/file.txt");
        assert!(g.regex.as_str().contains("(?P<DIR>[^/]+)"));
        assert_eq!(
            g.bound_captures.iter().map(|a| &**a).collect::<Vec<_>>(),
            vec!["DIR"]
        );
        let m = g.regex.captures(b"a/b/middle/file.txt").unwrap();
        assert_eq!(m.name("DIR").unwrap().as_bytes(), b"middle");
    }

    #[test]
    fn star_and_question_lower_to_non_slash_classes() {
        let g = compile("a*b?c");
        assert_eq!(g.regex.as_str(), r"^a[^/]*b[^/]c$");
    }

    #[test]
    fn term_ref_question_suffix_marks_unbound_mode() {
        use crate::value::TermMode;
        let g = compile("**/$X/$Y?");
        let modes: Vec<TermMode> = g.template.iter().filter_map(|s| match s {
            Seg::Term { mode, .. } => Some(*mode),
            _ => None,
        }).collect();
        assert_eq!(modes, vec![TermMode::Read, TermMode::Unbound]);
    }

    #[test]
    fn binds_captures_returns_hole_names() {
        let tree = parse_glob("**/$DIR/$FILE");
        let caps = GlobOp::default().binds_captures(&tree, b"**/$DIR/$FILE");
        let names: Vec<&str> = caps.iter().map(|a| a.as_ref()).collect();
        assert_eq!(names, vec!["DIR", "FILE"]);
    }

    #[test]
    fn try_raw_regex_exposes_compiled_source_for_bulk_backends() {
        let g = compile("**/*.rs");
        let op: Arc<dyn Op> = Arc::new(g);
        let raw = op.try_raw_regex().expect("glob exposes raw regex");
        assert!(raw.as_str().starts_with('^'));
        assert!(raw.as_str().ends_with('$'));
    }

    #[test]
    fn double_star_slash_matches_zero_or_more_segments() {
        let g = compile("src/**/*.rs");
        assert!(g.regex.is_match(b"src/foo.rs"));
        assert!(g.regex.is_match(b"src/a/foo.rs"));
        assert!(g.regex.is_match(b"src/a/b/foo.rs"));
        assert!(!g.regex.is_match(b"other/foo.rs"));
        assert!(!g.regex.is_match(b"README.md"));
    }

    #[test]
    fn leading_double_star_slash_matches_zero_prefix() {
        let g = compile("**/*.rs");
        assert!(g.regex.is_match(b"foo.rs"));
        assert!(g.regex.is_match(b"a/foo.rs"));
        assert!(g.regex.is_match(b"a/b/foo.rs"));
        assert!(!g.regex.is_match(b"foo.md"));
    }

    #[test]
    fn trailing_slash_double_star_matches_zero_suffix() {
        let g = compile("src/**");
        assert!(g.regex.is_match(b"src/lib.rs"));
        assert!(g.regex.is_match(b"src/a/b.rs"));
        assert!(g.regex.is_match(b"src"));
    }

    #[tokio::test]
    async fn apply_keeps_matches_drops_rest() {
        let g = compile("**/*.rs");
        let op: Arc<dyn Op> = Arc::new(g);
        let ctx = RtCtx::default();
        let mk = |bytes: &[u8]| Cursor::new(Arc::from(bytes));

        assert_eq!(op.pipe(&ctx, mk(b"src/lib.rs")).await.len(), 1);
        assert_eq!(op.pipe(&ctx, mk(b"deep/nested/path/mod.rs")).await.len(), 1);

        assert!(op.pipe(&ctx, mk(b"README.md")).await.is_empty());
        assert!(op.pipe(&ctx, mk(b"Cargo.toml")).await.is_empty());
    }
}
