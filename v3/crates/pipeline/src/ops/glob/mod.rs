//! `glob(...)` — path-glob pattern op (§14.2 / §14.7).
//!
//! Sub-grammar in `grammar.js` + committed `src/parser.c`. The Rust
//! side consumes the parsed injected tree in two ways:
//!
//!   - [`GlobOp::binds_captures`] walks `term_ref` nodes so the resolver
//!     can see `$DIR` / `$PATH` bindings at lower time (§19).
//!   - [`GlobOp::compile`] converts the injected tree into a
//!     [`CompiledGlob`]: a `Vec<Segment>` suitable for matching against
//!     a candidate path string.
//!
//! Matching itself is deferred to a separate `match_path` helper; the
//! `pattern_pipe` surface is a pass-through for this slice, letting
//! higher-level composition (`glob(...) > ...`) flow without wiring a
//! file-system walker through every test.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use sprefa_macros::sprf_pattern_op;
use tree_sitter::{Node, Tree};

use crate::_0_cursor::Cursor;
use crate::pattern_op::{CompiledPattern, PatternDiagnostic, PatternOp};

#[sprf_pattern_op(name = "glob")]
pub struct GlobOp;

/// One parsed glob segment.
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// `**` — match any sequence including `/`.
    DoubleStar,
    /// `*` — match any run of non-`/` bytes.
    Star,
    /// `?` — match exactly one non-`/` byte.
    Question,
    /// `/` — literal path separator.
    Slash,
    /// `$NAME` hole — binds the matched range to a capture.
    Hole { name: Arc<str> },
    /// Literal run of bytes.
    Literal(Arc<str>),
}

/// Output of [`GlobOp::compile`].
#[derive(Debug, Clone)]
pub struct CompiledGlob {
    pub segments: Vec<Segment>,
}

impl PatternOp for GlobOp {
    fn compile(
        &self,
        tree: &Tree,
        bytes: &[u8],
    ) -> Result<CompiledPattern, Vec<PatternDiagnostic>> {
        let root = tree.root_node();
        let mut segments = Vec::new();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            let seg = match child.kind() {
                "double_star" => Segment::DoubleStar,
                "star"        => Segment::Star,
                "question"    => Segment::Question,
                "slash"       => Segment::Slash,
                "term_ref"    => Segment::Hole {
                    name: term_ref_name(child, bytes),
                },
                "literal"     => Segment::Literal(
                    slice_to_arc_str(child, bytes),
                ),
                _ => continue,
            };
            segments.push(seg);
        }
        Ok(CompiledPattern::new("glob", CompiledGlob { segments }))
    }

    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>> {
        let root = tree.root_node();
        let mut out = Vec::new();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "term_ref" {
                out.push(term_ref_name(child, bytes));
            }
        }
        out
    }

    fn pattern_pipe<'a>(
        &'a self,
        _ctx: &'a RtCtx,
        c: Cursor,
    ) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { vec![c] })
    }
}

fn term_ref_name(node: Node<'_>, bytes: &[u8]) -> Arc<str> {
    let raw = std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("");
    Arc::from(raw.trim_start_matches('$'))
}

fn slice_to_arc_str(node: Node<'_>, bytes: &[u8]) -> Arc<str> {
    let raw = std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("");
    Arc::from(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_glob(src: &str) -> Tree {
        let mut p = Parser::new();
        let lang = crate::op_languages::language_of("glob")
            .expect("glob language registered");
        p.set_language(&lang).unwrap();
        p.parse(src, None).unwrap()
    }

    #[test]
    fn compile_plain_literal() {
        let src = "foo.txt";
        let tree = parse_glob(src);
        let compiled = GlobOp
            .compile(&tree, src.as_bytes())
            .unwrap();
        let g = compiled.downcast::<CompiledGlob>().unwrap();
        assert_eq!(g.segments.len(), 1);
        assert_eq!(g.segments[0], Segment::Literal(Arc::from("foo.txt")));
    }

    #[test]
    fn compile_with_hole_and_slash() {
        let src = "**/$DIR/file.txt";
        let tree = parse_glob(src);
        let g = GlobOp
            .compile(&tree, src.as_bytes())
            .unwrap();
        let g = g.downcast::<CompiledGlob>().unwrap();
        let kinds: Vec<&str> = g.segments.iter().map(|s| match s {
            Segment::DoubleStar   => "**",
            Segment::Star         => "*",
            Segment::Question     => "?",
            Segment::Slash        => "/",
            Segment::Hole { .. }  => "$",
            Segment::Literal(_)   => "lit",
        }).collect();
        assert_eq!(kinds, vec!["**", "/", "$", "/", "lit"]);
        match &g.segments[2] {
            Segment::Hole { name } => assert_eq!(&**name, "DIR"),
            _ => panic!("expected hole"),
        }
    }

    #[test]
    fn binds_captures_returns_hole_names() {
        let src = "**/$DIR/$FILE";
        let tree = parse_glob(src);
        let caps = GlobOp.binds_captures(&tree, src.as_bytes());
        let names: Vec<&str> = caps.iter().map(|a| a.as_ref()).collect();
        assert_eq!(names, vec!["DIR", "FILE"]);
    }

    #[test]
    fn compile_emits_stars_and_questions() {
        let src = "a*b?c";
        let tree = parse_glob(src);
        let g = GlobOp.compile(&tree, src.as_bytes()).unwrap();
        let g = g.downcast::<CompiledGlob>().unwrap();
        assert_eq!(g.segments.len(), 5);
        assert_eq!(g.segments[1], Segment::Star);
        assert_eq!(g.segments[3], Segment::Question);
    }
}
