//! `re(...)` — regular expression pattern op (§14.2 / §14.7).
//!
//! Grammar distinguishes `term_ref` ($NAME sugar) from everything else
//! (literal regex bytes). `compile` rewrites each `term_ref` into a
//! non-greedy named group `(?P<NAME>.*?)` and hands the concatenated
//! string to `regex::Regex`. Syntax errors in the regex body surface
//! as [`PatternDiagnostic`] with code `"pattern/re-syntax"`.
//!
//! `binds_captures` returns both the sugar names and the regex-native
//! named groups (`(?<X>...)` / `(?P<X>...)`) so downstream `> $NAME`
//! resolution treats the two origins identically (§14.7).

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use regex::Regex;
use sprefa_macros::sprf_pattern_op;
use tree_sitter::{Node, Tree};

use crate::_0_cursor::Cursor;
use crate::pattern_op::{CompiledPattern, PatternDiagnostic, PatternOp};

#[sprf_pattern_op(name = "re")]
pub struct ReOp;

#[derive(Debug, Clone)]
pub struct CompiledRe {
    pub regex: Regex,
    /// Desugared source — the string that was passed to `Regex::new`.
    pub desugared: Arc<str>,
}

impl PatternOp for ReOp {
    fn compile(
        &self,
        tree: &Tree,
        bytes: &[u8],
    ) -> Result<CompiledPattern, Vec<PatternDiagnostic>> {
        let desugared = desugar(tree, bytes);
        match Regex::new(&desugared) {
            Ok(regex) => Ok(CompiledPattern::new(
                "re",
                CompiledRe {
                    regex,
                    desugared: Arc::from(desugared),
                },
            )),
            Err(err) => Err(vec![PatternDiagnostic {
                code:       "pattern/re-syntax",
                message:    err.to_string(),
                byte_range: 0..bytes.len(),
            }]),
        }
    }

    fn binds_captures(&self, tree: &Tree, bytes: &[u8]) -> Vec<Arc<str>> {
        let mut out = Vec::new();

        // Sugar: $NAME nodes from the injected tree.
        let root = tree.root_node();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.kind() == "term_ref" {
                out.push(term_ref_name(child, bytes));
            }
        }

        // Native: named groups inside literal bytes. Compile the
        // desugared source to recover the final capture-name set, then
        // filter out the sugar names we already added.
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

    fn pattern_pipe<'a>(
        &'a self,
        _ctx: &'a RtCtx,
        c: Cursor,
    ) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move { vec![c] })
    }
}

fn desugar(tree: &Tree, bytes: &[u8]) -> String {
    let root = tree.root_node();
    let mut out = String::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "term_ref" => {
                let name = term_ref_name(child, bytes);
                out.push_str("(?P<");
                out.push_str(&name);
                out.push_str(">.*?)");
            }
            "literal" => {
                if let Ok(s) = std::str::from_utf8(&bytes[child.byte_range()]) {
                    out.push_str(s);
                }
            }
            _ => {}
        }
    }
    out
}

fn term_ref_name(node: Node<'_>, bytes: &[u8]) -> Arc<str> {
    let raw = std::str::from_utf8(&bytes[node.byte_range()]).unwrap_or("");
    Arc::from(raw.trim_start_matches('$'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn parse_re(src: &str) -> Tree {
        let mut p = Parser::new();
        let lang = crate::op_languages::language_of("re")
            .expect("re language registered");
        p.set_language(&lang).unwrap();
        p.parse(src, None).unwrap()
    }

    #[test]
    fn compile_literal_regex() {
        let src = r"TODO\(.+\)";
        let tree = parse_re(src);
        let c = ReOp.compile(&tree, src.as_bytes()).unwrap();
        let c = c.downcast::<CompiledRe>().unwrap();
        assert!(c.regex.is_match("TODO(fix this)"));
    }

    #[test]
    fn compile_desugars_term_ref_to_named_group() {
        let src = r"TODO\($WHO\)";
        let tree = parse_re(src);
        let c = ReOp.compile(&tree, src.as_bytes()).unwrap();
        let c = c.downcast::<CompiledRe>().unwrap();
        assert!(c.desugared.contains("(?P<WHO>.*?)"));
        let caps = c.regex.captures("TODO(alice)").unwrap();
        assert_eq!(caps.name("WHO").unwrap().as_str(), "alice");
    }

    #[test]
    fn binds_captures_reports_sugar_and_native_groups() {
        let src = r"$FIRST (?P<SECOND>\w+)";
        let tree = parse_re(src);
        let names: Vec<String> = ReOp
            .binds_captures(&tree, src.as_bytes())
            .into_iter()
            .map(|a| a.to_string())
            .collect();
        // Sugar appears first, then native (if not duplicated).
        assert!(names.contains(&"FIRST".to_string()));
        assert!(names.contains(&"SECOND".to_string()));
    }

    #[test]
    fn compile_returns_diag_on_bad_regex() {
        let src = "[unclosed";
        let tree = parse_re(src);
        let e = ReOp.compile(&tree, src.as_bytes()).unwrap_err();
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].code, "pattern/re-syntax");
    }
}
