//! `json(<jq_path>, ${CAP?})` — DEPRECATED jq-path v0.
//!
//! NOT REGISTERED in `Registry::with_stdlib()`. Wrong direction per
//! user feedback: the target is v2's brace-walker DSL with `**` wildcards
//! and structural multi-capture (`json({ name: $N, version: $V })`,
//! `json({ **: { image: $I } })`). This file is parked pending the
//! brace-walker port (see sprefa-lpk).
//!
//! Original v0 doc preserved below for reference.
//! ----
//!
//! `json(<jq_path>, ${CAP?})` — extract a single value from a JSON
//! document by jq-style path, bind it to a capture.
//!
//! v0 scope (sprefa-4m7.2.12 first slice):
//!   * Single jq path → single Unbound capture.
//!   * Path grammar: dot-separated keys, bracketed integer indices, `[]`
//!     for array fan-out. Examples: `.name`, `.version`, `.deps.foo`,
//!     `.items[0]`, `.items[]` (fan-out).
//!   * Format support: JSON only (uses `tree-sitter-json`). YAML/TOML
//!     deferred — v2 had them via a generic data-tree abstraction
//!     (~2000 LoC dependency cone) that does not belong in a port pass.
//!   * Brace-pattern multi-capture form (`json({k1:$X, k2:$Y})`) deferred.
//!     The brace walker is its own subsystem; landing it requires an
//!     active use case to scope.
//!
//! Cursor flow:
//!   * `cursor.content` is parsed with tree-sitter-json on first call.
//!   * For each match of the jq path, an output cursor is emitted with
//!     `byte_range` narrowed to the matched value's bytes and a new
//!     `Capture` named after the introducer.
//!   * `[]` array-fan-out emits one cursor per array element.
//!
//! Diagnostics:
//!   * Missing args                              → `json/missing-args`
//!   * Wrong arg shapes                          → `value/wrong-kind`
//!   * Capture not Unbound                       → `term/must-be-unbound-in-json`
//!   * jq path syntax error                      → `json/path-syntax`
//!   * tree-sitter-json parse error              → `json/parse`
//!
//! Spec: parse.md §14 (data DSLs); supersedes v2/src/ops/_5_json.rs for
//! the single-path slice.

use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::{Node, Parser};

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::{TermMode, Value};

#[derive(Debug, Clone)]
pub enum PathSeg {
    Key(Arc<str>),
    Index(usize),
    /// `[]` — fan out across all elements of an array.
    AllItems,
}

#[derive(Debug)]
pub struct JsonOp {
    pub path:    Vec<PathSeg>,
    pub capture: Arc<str>,
}

impl OpCtor for JsonOp {
    const NAME: &'static str = "json";
    const BODY_GRAMMAR: &'static str = "jq-path string + ${CAP?} introducer";
    const DOC:  &'static str = "\
**json**(_path_, ${CAP?})

Extract values from a JSON document by jq-style path. Path grammar:
dot-keys (`.name`), bracketed integer indices (`.items[0]`), and array
fan-out (`.items[]` emits one cursor per element).

Each match emits one cursor with `byte_range` narrowed to the value's
span and `${CAP}` bound to the same span. v0: single path + single
introducer; brace-pattern multi-capture form is parked.
";

    fn from_values(values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        if values.len() != 2 {
            return Err(vec![PatternDiagnostic {
                code: "json/missing-args",
                message: "json takes exactly two arguments: a jq-path string and an Unbound `${CAP?}` capture".into(),
                byte_range: 0..0,
            }]);
        }

        let path = match &values[0] {
            Value::Str(s) | Value::Atom(s) => parse_jq_path(s)?,
            _ => {
                return Err(vec![PatternDiagnostic {
                    code: "value/wrong-kind",
                    message: "json's first arg must be a jq-path string literal".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        let capture = match &values[1] {
            Value::Term { name, mode: TermMode::Unbound } => name.clone(),
            Value::Term { name, mode: TermMode::Read } => {
                return Err(vec![PatternDiagnostic {
                    code: "term/must-be-unbound-in-json",
                    message: format!(
                        "`${{{name}}}` must be `${{{name}?}}` — json binds the path's value into a fresh capture"
                    ),
                    byte_range: 0..0,
                }]);
            }
            _ => {
                return Err(vec![PatternDiagnostic {
                    code: "value/wrong-kind",
                    message: "json's second arg must be an Unbound `${CAP?}` term".into(),
                    byte_range: 0..0,
                }]);
            }
        };

        Ok(JsonOp { path, capture })
    }
}

impl Op for JsonOp {
    fn name(&self) -> &'static str { "json" }

    fn pipe<'a>(&'a self, ctx: &'a RtCtx, c: Cursor) -> BoxFuture<'a, Vec<Cursor>> {
        Box::pin(async move {
            let Some(c) = crate::effects::ensure_content_loaded(ctx, c).await else {
                return Vec::new();
            };
            let active = c.active();
            let base = c.byte_range.start;

            let mut parser = Parser::new();
            if parser
                .set_language(&tree_sitter_json::LANGUAGE.into())
                .is_err()
            {
                return Vec::new();
            }
            let Some(tree) = parser.parse(active, None) else {
                return Vec::new();
            };

            let root = tree.root_node();
            // tree-sitter-json's root is "document"; the value is the
            // first named child.
            let value_root = root.named_child(0).unwrap_or(root);

            let mut hits: Vec<(usize, usize)> = Vec::new();
            walk_path(value_root, active, &self.path, &mut hits);

            let mut out = Vec::with_capacity(hits.len());
            for (start, end) in hits {
                let abs = (base + start)..(base + end);
                let mut next = c.narrow(abs.clone());
                next.captures
                    .push(Capture::span_backed(self.capture.clone(), abs));
                out.push(next);
            }
            out
        })
    }

    fn bound_captures(&self) -> &[Arc<str>] {
        std::slice::from_ref(&self.capture)
    }
}

// ---------------------------------------------------------------------------
// jq path parser — `.key`, `.key.sub`, `.items[0]`, `.items[]`.
// ---------------------------------------------------------------------------

fn parse_jq_path(src: &str) -> Result<Vec<PathSeg>, Vec<PatternDiagnostic>> {
    let s = src.trim();
    if s.is_empty() || s == "." {
        return Ok(Vec::new());
    }
    if !s.starts_with('.') {
        return Err(vec![PatternDiagnostic {
            code: "json/path-syntax",
            message: format!("jq path must start with `.`, got `{s}`"),
            byte_range: 0..0,
        }]);
    }
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'.' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'.' && bytes[i] != b'[' {
                    i += 1;
                }
                if i > start {
                    let key = std::str::from_utf8(&bytes[start..i])
                        .map_err(|_| {
                            vec![PatternDiagnostic {
                                code: "json/path-syntax",
                                message: "non-utf8 jq key".into(),
                                byte_range: 0..0,
                            }]
                        })?;
                    out.push(PathSeg::Key(Arc::from(key)));
                }
            }
            b'[' => {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i >= bytes.len() {
                    return Err(vec![PatternDiagnostic {
                        code: "json/path-syntax",
                        message: "unterminated `[` in jq path".into(),
                        byte_range: 0..0,
                    }]);
                }
                let inner =
                    std::str::from_utf8(&bytes[start..i]).map_err(|_| vec![
                        PatternDiagnostic {
                            code: "json/path-syntax",
                            message: "non-utf8 jq index".into(),
                            byte_range: 0..0,
                        },
                    ])?;
                if inner.is_empty() {
                    out.push(PathSeg::AllItems);
                } else if let Ok(n) = inner.parse::<usize>() {
                    out.push(PathSeg::Index(n));
                } else {
                    return Err(vec![PatternDiagnostic {
                        code: "json/path-syntax",
                        message: format!("jq index must be empty (`[]`) or integer, got `{inner}`"),
                        byte_range: 0..0,
                    }]);
                }
                i += 1; // skip ']'
            }
            _ => {
                return Err(vec![PatternDiagnostic {
                    code: "json/path-syntax",
                    message: format!("unexpected byte `{}` in jq path", bytes[i] as char),
                    byte_range: 0..0,
                }]);
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Tree walker — applies a path to a tree-sitter-json node, collecting
// the matched values' byte ranges.
// ---------------------------------------------------------------------------

fn walk_path(
    node: Node<'_>,
    src:  &[u8],
    path: &[PathSeg],
    out:  &mut Vec<(usize, usize)>,
) {
    let Some(seg) = path.first() else {
        out.push((node.start_byte(), node.end_byte()));
        return;
    };
    let rest = &path[1..];

    match seg {
        PathSeg::Key(k) => {
            // Object: walk children with kind == "pair".
            if node.kind() != "object" {
                return;
            }
            let mut walk = node.walk();
            for child in node.named_children(&mut walk) {
                if child.kind() != "pair" {
                    continue;
                }
                let Some(key_node) = child.child_by_field_name("key") else {
                    continue;
                };
                let key_text = node_string_text(key_node, src);
                if key_text.as_deref() == Some(k.as_ref()) {
                    if let Some(val) = child.child_by_field_name("value") {
                        walk_path(val, src, rest, out);
                    }
                }
            }
        }
        PathSeg::Index(n) => {
            if node.kind() != "array" {
                return;
            }
            let items: Vec<Node> = (0..node.named_child_count())
                .filter_map(|i| node.named_child(i))
                .collect();
            if let Some(item) = items.get(*n) {
                walk_path(*item, src, rest, out);
            }
        }
        PathSeg::AllItems => {
            if node.kind() != "array" {
                return;
            }
            let mut walk = node.walk();
            for child in node.named_children(&mut walk) {
                walk_path(child, src, rest, out);
            }
        }
    }
}

/// Return the unquoted text of a tree-sitter-json `string` node, or
/// `None` if the node is not a string. tree-sitter-json strings expose
/// the raw bytes including surrounding quotes; we strip them here.
fn node_string_text(node: Node<'_>, src: &[u8]) -> Option<String> {
    if node.kind() != "string" {
        return None;
    }
    let raw = std::str::from_utf8(&src[node.byte_range()]).ok()?;
    let trimmed = raw.trim();
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"' {
        Some(trimmed[1..trimmed.len() - 1].to_string())
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(path: &str, cap: &str) -> JsonOp {
        JsonOp::from_values(vec![
            Value::Str(Arc::from(path)),
            Value::Term {
                name: Arc::from(cap),
                mode: TermMode::Unbound,
            },
        ])
        .unwrap()
    }

    #[test]
    fn path_parse_simple_keys() {
        let p = parse_jq_path(".name").unwrap();
        assert!(matches!(p[0], PathSeg::Key(ref k) if &**k == "name"));
    }

    #[test]
    fn path_parse_nested_with_index_and_fanout() {
        let p = parse_jq_path(".deps.list[0]").unwrap();
        assert_eq!(p.len(), 3);
        assert!(matches!(p[2], PathSeg::Index(0)));

        let p = parse_jq_path(".items[]").unwrap();
        assert_eq!(p.len(), 2);
        assert!(matches!(p[1], PathSeg::AllItems));
    }

    #[test]
    fn path_parse_rejects_no_dot() {
        let e = parse_jq_path("name").unwrap_err();
        assert_eq!(e[0].code, "json/path-syntax");
    }

    #[tokio::test]
    async fn extract_top_level_string_field() {
        let ctx = RtCtx::default();
        let body = br#"{"name": "alice", "version": "1.2"}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let out = op(".version", "V").pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        // Match span includes quotes.
        assert_eq!(out[0].active(), b"\"1.2\"");
        let cap = out[0]
            .captures
            .iter()
            .find(|c| &*c.name == "V")
            .unwrap();
        assert_eq!(cap.byte_range, out[0].byte_range);
    }

    #[tokio::test]
    async fn extract_nested_field() {
        let ctx = RtCtx::default();
        let body = br#"{"deps": {"foo": "1.0"}}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let out = op(".deps.foo", "X").pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].active(), b"\"1.0\"");
    }

    #[tokio::test]
    async fn array_index_extracts_one() {
        let ctx = RtCtx::default();
        let body = br#"{"items": ["a", "b", "c"]}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let out = op(".items[1]", "X").pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].active(), b"\"b\"");
    }

    #[tokio::test]
    async fn array_fanout_emits_one_per_element() {
        let ctx = RtCtx::default();
        let body = br#"{"items": [1, 2, 3]}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let out = op(".items[]", "X").pipe(&ctx, c).await;
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].active(), b"1");
        assert_eq!(out[1].active(), b"2");
        assert_eq!(out[2].active(), b"3");
    }

    #[tokio::test]
    async fn missing_path_emits_no_cursors() {
        let ctx = RtCtx::default();
        let body = br#"{"name": "alice"}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let out = op(".version", "X").pipe(&ctx, c).await;
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn from_values_rejects_read_term() {
        let err = JsonOp::from_values(vec![
            Value::Str(Arc::from(".x")),
            Value::Term {
                name: Arc::from("Y"),
                mode: TermMode::Read,
            },
        ])
        .unwrap_err();
        assert_eq!(err[0].code, "term/must-be-unbound-in-json");
    }

    #[test]
    fn from_values_rejects_wrong_arity() {
        let err = JsonOp::from_values(vec![Value::Str(Arc::from(".x"))]).unwrap_err();
        assert_eq!(err[0].code, "json/missing-args");
    }
}
