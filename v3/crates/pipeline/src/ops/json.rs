//! `json({ ... })` — brace-walker DSL over JSON.
//!
//! Ported from v2/src/ops/_5_json.rs + v2 walk subsystem. Body is a
//! curly-brace pattern with structural matching: bare keys match exactly,
//! `$NAME` captures values, `$KEY` in key position captures keys, `**`
//! recurses through nested objects, `[...]` iterates arrays, and quoted
//! strings with `$VARS` are leaf-segment patterns.
//!
//! Examples:
//!   json({ name: $N, version: $V })
//!   json({ deps: { $K: $V } })
//!   json({ **: { image: "$REPO:$TAG" } })
//!   json({ users: [...{ name: $N, age: $A }] })
//!
//! Cursor flow: each WalkOutcome row becomes one output cursor; capture
//! names map to v3 `Capture::span_backed` entries with absolute byte
//! ranges (cursor.byte_range.start + walker offset).
//!
//! YAML/TOML deferred (sprefa-lpk follow-up). The v2 `DataNode` trait
//! lives in `crate::data` so additional formats slot in without walker
//! changes.

use std::ops::Range;
use std::sync::Arc;

use effect_runtime::{BoxFuture, RtCtx};
use tree_sitter::Node;

use crate::_0_cursor::{Capture, Cursor};
use crate::_1_op::Op;
use crate::data::JsonNode;
use crate::op_ctor::OpCtor;
use crate::pattern_op::PatternDiagnostic;
use crate::value::Value;
use crate::walk::brace_parse::{parse_body, ScanAnnotation};
use crate::walk::compile::compile_steps;
use crate::walk::compiled::{CompiledKeyMatcher, CompiledStep};
use crate::walk::walker::walk;

/// Compiled `json({...})` op.
///
/// `_annotations` (from v2's `$$sigil($X)` form) parse but are not
/// surfaced today — kept for fidelity until sprefa-4m7.13.x decides
/// whether v3 cross-repo wiring revives them.
#[derive(Debug)]
pub struct JsonOp {
    pub steps:           Arc<[CompiledStep]>,
    pub bound_caps:      Vec<Arc<str>>,
    pub _annotations:    Arc<[ScanAnnotation]>,
}

impl OpCtor for JsonOp {
    const NAME: &'static str = "json";
    const BODY_GRAMMAR: &'static str = "brace pattern: { key: $V, $K: ..., **: ... }";
    const DOC: &'static str = "\
**json**({ _pattern_ })

Brace-pattern walker over JSON documents. Body is a structural pattern:

- bare keys match exactly: `{ name: $N }`
- captured keys: `{ $K: $V }`
- `**` recurses through nested objects: `{ **: { image: $I } }`
- arrays via `[...]`: `{ items: [...{ id: $ID }] }`
- quoted leaf patterns: `{ image: \"$REPO:$TAG\" }`
- regex / glob keys: `{ re:^dep_: $V }`, `{ \"@$SCOPE/$NAME\": $V }`

Each match emits one cursor with named captures bound to the matched
spans. Multiple captures fan out as a row-per-match.
";

    fn from_values(_values: Vec<Value>) -> Result<Self, Vec<PatternDiagnostic>> {
        // Defensive: registry always routes json through `from_paren_node`.
        // If a caller bypasses the node path, return an empty walker that
        // matches nothing. Tests/static callers should use `lower_body`.
        Ok(JsonOp {
            steps:        Arc::from(Vec::<CompiledStep>::new()),
            bound_caps:   Vec::new(),
            _annotations: Arc::from(Vec::<ScanAnnotation>::new()),
        })
    }

    fn from_paren_node(
        inv_node: Node<'_>,
        src:      &[u8],
    ) -> Option<Result<Self, Vec<PatternDiagnostic>>> {
        Some(lower_json_paren(inv_node, src))
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

            let Ok(json) = JsonNode::parse(Arc::from(active)) else {
                return Vec::new();
            };

            let outcome = walk(&json, &self.steps);

            let mut out = Vec::with_capacity(outcome.rows.len());
            for row in outcome.rows {
                let mut next = c.clone();
                let mut union_start: Option<usize> = None;
                let mut union_end:   Option<usize> = None;
                for (name, wc) in row.captures {
                    let abs: Range<usize> =
                        (base + wc.byte_start as usize)..(base + wc.byte_end as usize);
                    union_start = Some(union_start.map(|s| s.min(abs.start)).unwrap_or(abs.start));
                    union_end   = Some(union_end.map(|e| e.max(abs.end)).unwrap_or(abs.end));
                    next.captures.push(Capture::span_backed(name, abs));
                }
                if let (Some(s), Some(e)) = (union_start, union_end) {
                    next.byte_range = s..e;
                }
                out.push(next);
            }
            out
        })
    }

    fn bound_captures(&self) -> &[Arc<str>] { &self.bound_caps }
}

// ---------------------------------------------------------------------------
// Lowering: host CST `op_invocation` → JsonOp.
//
// json's body is the paren slot's bytes between `(` and `)`. We grab
// those bytes, run the v2 brace parser, compile to CompiledSteps. The
// v3 host grammar's term_ref / carveout_expr children inside the paren
// get ignored — the brace parser owns the byte-level DSL.
// ---------------------------------------------------------------------------

fn lower_json_paren(
    inv_node: Node<'_>,
    src:      &[u8],
) -> Result<JsonOp, Vec<PatternDiagnostic>> {
    let Some(paren) = inv_node.child_by_field_name("paren") else {
        return Err(vec![PatternDiagnostic {
            code: "json/missing-body",
            message: "json requires a brace-pattern body, e.g. json({ key: $V })".into(),
            byte_range: inv_node.byte_range(),
        }]);
    };
    let p_start = paren.start_byte();
    let p_end   = paren.end_byte();
    let body_bytes = if p_end < p_start + 2 { &[][..] } else { &src[p_start + 1..p_end - 1] };
    let body = std::str::from_utf8(body_bytes).map_err(|_| {
        vec![PatternDiagnostic {
            code: "json/non-utf8",
            message: "json body is not valid UTF-8".into(),
            byte_range: paren.byte_range(),
        }]
    })?;
    if body.trim().is_empty() {
        return Err(vec![PatternDiagnostic {
            code: "json/empty-body",
            message: "json body is empty; expected a brace pattern, e.g. json({ key: $V })".into(),
            byte_range: paren.byte_range(),
        }]);
    }

    let (steps, annotations) = parse_body(body).map_err(|e| {
        vec![PatternDiagnostic {
            code: "json/parse-body",
            message: e,
            byte_range: paren.byte_range(),
        }]
    })?;

    let compiled = compile_steps(&steps).map_err(|e| {
        vec![PatternDiagnostic {
            code: "json/compile",
            message: e,
            byte_range: paren.byte_range(),
        }]
    })?;

    let bound_caps = collect_pattern_captures(&compiled);

    Ok(JsonOp {
        steps:        Arc::from(compiled.into_boxed_slice()),
        bound_caps,
        _annotations: Arc::from(annotations.into_boxed_slice()),
    })
}

fn collect_pattern_captures(steps: &[CompiledStep]) -> Vec<Arc<str>> {
    let mut seen: Vec<Arc<str>> = Vec::new();
    fn push(seen: &mut Vec<Arc<str>>, name: &str) {
        if !seen.iter().any(|s| s.as_ref() == name) {
            seen.push(Arc::from(name));
        }
    }
    fn walk_steps(seen: &mut Vec<Arc<str>>, steps: &[CompiledStep]) {
        for s in steps {
            match s {
                CompiledStep::Key { capture: Some(n), .. }
                | CompiledStep::KeyMatch { capture: Some(n), .. }
                | CompiledStep::Leaf { capture: Some(n) }
                | CompiledStep::CaptureAny { capture: Some(n) } => push(seen, n),
                CompiledStep::Object { entries } => {
                    for e in entries {
                        if let CompiledKeyMatcher::Capture(n) = &e.key { push(seen, n); }
                        walk_steps(seen, &e.value);
                    }
                }
                CompiledStep::Array { item } => walk_steps(seen, item),
                _ => {}
            }
        }
    }
    walk_steps(&mut seen, steps);
    seen
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn host_lang() -> tree_sitter::Language {
        tree_sitter_sprefa::LANGUAGE.into()
    }

    fn lower(src: &str) -> Result<JsonOp, Vec<PatternDiagnostic>> {
        let mut parser = Parser::new();
        parser.set_language(&host_lang()).unwrap();
        let tree = parser.parse(src, None).unwrap();
        let root = tree.root_node();
        let inv = find_op_invocation(root).expect("no op_invocation in source");
        lower_json_paren(inv, src.as_bytes())
    }

    fn find_op_invocation(n: Node<'_>) -> Option<Node<'_>> {
        if n.kind() == "op_invocation" { return Some(n); }
        let mut walk = n.walk();
        for ch in n.named_children(&mut walk) {
            if let Some(v) = find_op_invocation(ch) { return Some(v); }
        }
        None
    }

    #[test]
    fn lower_flat_pattern_collects_caps() {
        let op = lower("json({ name: $N, version: $V })").unwrap();
        let names: Vec<&str> = op.bound_captures().iter().map(|s| s.as_ref()).collect();
        assert!(names.contains(&"N"));
        assert!(names.contains(&"V"));
    }

    #[test]
    fn lower_rejects_empty_body() {
        let err = lower("json()").unwrap_err();
        assert_eq!(err[0].code, "json/empty-body");
    }

    #[test]
    fn lower_rejects_bad_brace_syntax() {
        let err = lower("json({ name }").unwrap_err();
        assert_eq!(err[0].code, "json/parse-body");
    }

    #[tokio::test]
    async fn pipe_extracts_flat_object() {
        let ctx = RtCtx::default();
        let body = br#"{"name":"alice","version":"1.2"}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("json({ name: $N, version: $V })").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        let n = out[0].captures.iter().find(|c| &*c.name == "N").unwrap();
        let v = out[0].captures.iter().find(|c| &*c.name == "V").unwrap();
        assert_eq!(n.bytes(&out[0].content), b"\"alice\"");
        assert_eq!(v.bytes(&out[0].content), b"\"1.2\"");
    }

    #[tokio::test]
    async fn pipe_captured_keys_fan_out() {
        let ctx = RtCtx::default();
        let body = br#"{"deps":{"foo":"1","bar":"2"}}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("json({ deps: { $K: $V } })").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
        let mut pairs: Vec<(String, String)> = out
            .iter()
            .map(|c| {
                let k = c.captures.iter().find(|c| &*c.name == "K").unwrap();
                let v = c.captures.iter().find(|c| &*c.name == "V").unwrap();
                (
                    String::from_utf8_lossy(k.bytes(&c.content)).into_owned(),
                    String::from_utf8_lossy(v.bytes(&c.content)).into_owned(),
                )
            })
            .collect();
        pairs.sort();
        assert_eq!(
            pairs,
            vec![
                ("\"bar\"".to_string(), "\"2\"".to_string()),
                ("\"foo\"".to_string(), "\"1\"".to_string()),
            ]
        );
    }

    #[tokio::test]
    async fn pipe_array_iter() {
        let ctx = RtCtx::default();
        let body = br#"{"users":[{"id":"a"},{"id":"b"}]}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("json({ users: [...{ id: $I }] })").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 2);
    }

    #[tokio::test]
    async fn pipe_double_star_recurses() {
        let ctx = RtCtx::default();
        let body = br#"{"a":{"b":{"image":"nginx"}}}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower("json({ **: { image: $I } })").unwrap();
        let out = op.pipe(&ctx, c).await;
        assert!(out.iter().any(|c| {
            c.captures.iter().any(|cap| &*cap.name == "I" && cap.bytes(&c.content) == b"\"nginx\"")
        }));
    }

    #[tokio::test]
    async fn pipe_leaf_pattern_splits_image_tag() {
        let ctx = RtCtx::default();
        let body = br#"{"image":"nginx:1.21"}"#;
        let c = Cursor::new(Arc::from(body.as_slice()));
        let op = lower(r#"json({ image: "$REPO:$TAG" })"#).unwrap();
        let out = op.pipe(&ctx, c).await;
        assert_eq!(out.len(), 1);
        let r = out[0].captures.iter().find(|c| &*c.name == "REPO").unwrap();
        let t = out[0].captures.iter().find(|c| &*c.name == "TAG").unwrap();
        assert_eq!(String::from_utf8_lossy(r.bytes(&out[0].content)), "\"nginx:1.21\"");
        assert_eq!(String::from_utf8_lossy(t.bytes(&out[0].content)), "\"nginx:1.21\"");
    }
}
