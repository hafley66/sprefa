//! Data walker: evaluates CompiledSteps against a DataNode tree.
//!
//! Ported from v1 `walk_inner` in `crates/rules/src/walk.rs`, with two changes:
//!   1. Source of truth is a `DataNode` (tree-sitter backed), not serde_json::Value.
//!      Captures carry real byte ranges.
//!   2. Object{entries} uses row-split semantics: sibling entries that capture
//!      in separate subtrees emit separate rows (with NULLs for disjoint columns);
//!      sibling entries that capture at the immediate-value level (row-field
//!      destructure) still join into one row.
//!
//! Row-split classification (per entry in Object{entries}):
//!   - entry.value = [Leaf{..}] | [LeafPattern{..}]  -> row field (joins)
//!   - everything else                               -> descent (concat)

use std::borrow::Cow;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::_0_pattern::match_segments_with_bindings;
use super::_1_compiled::{
    CompiledKeyMatcher, CompiledObjectEntry, CompiledStep, WalkCapture,
};
use crate::data::{DataKind, DataNode};
use crate::jq_path::{push_index, push_key};

pub type Captures = FxHashMap<Arc<str>, WalkCapture>;

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub captures: Captures,
}

#[derive(Clone)]
struct Ctx {
    depth: u32,
    parent_key: Option<Arc<str>>,
    path: String,
}

impl Ctx {
    fn new() -> Self {
        Self { depth: 0, parent_key: None, path: String::new() }
    }

    fn descend_key(&self, key: &str) -> Self {
        let mut path = self.path.clone();
        push_key(&mut path, key);
        Self { depth: self.depth + 1, parent_key: Some(Arc::from(key)), path }
    }

    fn descend_index(&self, i: u32) -> Self {
        let mut path = self.path.clone();
        push_index(&mut path, i);
        Self { depth: self.depth + 1, parent_key: None, path }
    }
}

pub fn walk<N: DataNode>(root: &N, steps: &[CompiledStep]) -> Vec<MatchResult> {
    walk_with_captures(root, steps, FxHashMap::default())
}

pub fn walk_with_captures<N: DataNode>(
    root: &N,
    steps: &[CompiledStep],
    seed: Captures,
) -> Vec<MatchResult> {
    walk_inner(root, steps, &Ctx::new(), &seed)
}

fn walk_inner<N: DataNode>(
    node: &N,
    steps: &[CompiledStep],
    ctx: &Ctx,
    caps: &Captures,
) -> Vec<MatchResult> {
    if steps.is_empty() {
        return vec![MatchResult { captures: caps.clone() }];
    }

    let step = &steps[0];
    let rest = &steps[1..];

    match step {
        CompiledStep::Any => {
            let mut out = walk_inner(node, rest, ctx, caps);
            match node.kind() {
                DataKind::Object => {
                    for (k, v) in node.entries() {
                        let k_text = scalar_text(&k);
                        let child_ctx = ctx.descend_key(&k_text);
                        out.extend(walk_inner(&v, steps, &child_ctx, caps));
                    }
                }
                DataKind::Array => {
                    for (i, v) in node.items().enumerate() {
                        let child_ctx = ctx.descend_index(i as u32);
                        out.extend(walk_inner(&v, steps, &child_ctx, caps));
                    }
                }
                _ => {}
            }
            out
        }

        CompiledStep::Key { name, capture } => {
            if node.kind() != DataKind::Object {
                return vec![];
            }
            for (k, v) in node.entries() {
                let k_text = scalar_text(&k);
                if k_text == *name {
                    let child_ctx = ctx.descend_key(&k_text);
                    let mut next_caps = caps.clone();
                    if let Some(cap) = capture {
                        let (ks, ke) = k.byte_range();
                        next_caps.insert(
                            Arc::from(cap.as_str()),
                            WalkCapture {
                                text: Arc::from(k_text),
                                path: Arc::from(child_ctx.path.as_str()),
                                byte_start: ks,
                                byte_end:   ke,
                            },
                        );
                    }
                    return walk_inner(&v, rest, &child_ctx, &next_caps);
                }
            }
            vec![]
        }

        CompiledStep::KeyMatch { matchers, capture } => {
            if node.kind() != DataKind::Object {
                return vec![];
            }
            let mut out = vec![];
            for (k, v) in node.entries() {
                let k_text = scalar_text(&k);
                if matchers.iter().any(|m| m.is_match(&k_text)) {
                    let child_ctx = ctx.descend_key(&k_text);
                    let mut next_caps = caps.clone();
                    if let Some(cap) = capture {
                        let (ks, ke) = k.byte_range();
                        next_caps.insert(
                            Arc::from(cap.as_str()),
                            WalkCapture {
                                text: Arc::from(k_text.as_str()),
                                path: Arc::from(child_ctx.path.as_str()),
                                byte_start: ks,
                                byte_end:   ke,
                            },
                        );
                    }
                    out.extend(walk_inner(&v, rest, &child_ctx, &next_caps));
                }
            }
            out
        }

        CompiledStep::DepthMin { n } => {
            if ctx.depth >= *n { walk_inner(node, rest, ctx, caps) } else { vec![] }
        }
        CompiledStep::DepthMax { n } => {
            if ctx.depth <= *n { walk_inner(node, rest, ctx, caps) } else { vec![] }
        }
        CompiledStep::DepthEq { n } => {
            if ctx.depth == *n { walk_inner(node, rest, ctx, caps) } else { vec![] }
        }

        CompiledStep::ParentKey { matchers } => match &ctx.parent_key {
            Some(pk) if matchers.iter().any(|m| m.is_match(pk)) => {
                walk_inner(node, rest, ctx, caps)
            }
            _ => vec![],
        },

        CompiledStep::ArrayItem => {
            if node.kind() != DataKind::Array {
                return vec![];
            }
            let mut out = vec![];
            for (i, v) in node.items().enumerate() {
                let child_ctx = ctx.descend_index(i as u32);
                out.extend(walk_inner(&v, rest, &child_ctx, caps));
            }
            out
        }

        CompiledStep::Leaf { capture } => {
            let Some(text) = node.as_scalar_text() else {
                return vec![];
            };
            let mut next_caps = caps.clone();
            if let Some(cap) = capture {
                let (bs, be) = node.byte_range();
                next_caps.insert(
                    Arc::from(cap.as_str()),
                    WalkCapture {
                        text: Arc::from(text.as_ref()),
                        path: Arc::from(ctx.path.as_str()),
                        byte_start: bs,
                        byte_end:   be,
                    },
                );
            }
            walk_inner(node, rest, ctx, &next_caps)
        }

        CompiledStep::LeafPattern { segments } => {
            let Some(text) = node.as_scalar_text() else {
                return vec![];
            };
            // Pre-seed with existing captures (treated as constraints).
            let pre_bound: std::collections::HashMap<String, String> = caps
                .iter()
                .map(|(k, cv)| (k.to_string(), cv.text.to_string()))
                .collect();
            let Some(seg_caps) = match_segments_with_bindings(segments, &text, pre_bound) else {
                return vec![];
            };
            let mut next_caps = caps.clone();
            let (bs, be) = node.byte_range();
            for (name, value) in seg_caps {
                // Only insert if this name wasn't pre-bound (new capture, not constraint).
                if !caps.contains_key(name.as_str()) {
                    next_caps.insert(
                        Arc::from(name.as_str()),
                        WalkCapture {
                            text: Arc::from(value.as_str()),
                            path: Arc::from(ctx.path.as_str()),
                            byte_start: bs,
                            byte_end:   be,
                        },
                    );
                }
            }
            walk_inner(node, rest, ctx, &next_caps)
        }

        CompiledStep::Object { entries } => {
            if node.kind() != DataKind::Object {
                return vec![];
            }
            walk_object_entries(node, entries, rest, ctx, caps)
        }

        CompiledStep::Array { item } => {
            if node.kind() != DataKind::Array {
                return vec![];
            }
            let mut out = vec![];
            for (i, v) in node.items().enumerate() {
                let child_ctx = ctx.descend_index(i as u32);
                let sub = walk_inner(&v, item, &child_ctx, caps);
                for r in sub {
                    out.extend(walk_inner(node, rest, ctx, &r.captures));
                }
            }
            out
        }
    }
}

// ---------------------------------------------------------------------------
// Object{entries} with row-split semantics
// ---------------------------------------------------------------------------

fn is_row_field(entry: &CompiledObjectEntry) -> bool {
    entry.value.len() == 1 && matches!(
        entry.value[0],
        CompiledStep::Leaf { .. } | CompiledStep::LeafPattern { .. }
    )
}

fn walk_object_entries<N: DataNode>(
    node: &N,
    entries: &[CompiledObjectEntry],
    rest: &[CompiledStep],
    ctx: &Ctx,
    caps: &Captures,
) -> Vec<MatchResult> {
    // Split entries by role. Preserving order is not semantically required
    // (Object is unordered), but it keeps diagnostics stable.
    let mut row_fields: Vec<&CompiledObjectEntry> = vec![];
    let mut descents:   Vec<&CompiledObjectEntry> = vec![];
    for e in entries {
        if is_row_field(e) { row_fields.push(e); } else { descents.push(e); }
    }

    // Build row templates by walking row_fields with cross-product (one or more
    // field contributions merged into a single row per key match). Partial
    // semantics: a row_field that finds zero matching keys (or whose walks all
    // fail) is tolerated — the other entries still produce rows, and the
    // missing capture simply stays unbound. Missing-entry diagnostics are
    // surfaced by the caller if desired.
    let mut row_templates: Vec<Captures> = vec![caps.clone()];
    for entry in &row_fields {
        let key_hits = resolve_keys(node, &entry.key);
        if key_hits.is_empty() { continue; }
        let mut next_templates = vec![];
        for tmpl in &row_templates {
            for (k_text, k_range, v_node) in &key_hits {
                let child_ctx = ctx.descend_key(k_text);
                let mut seeded = tmpl.clone();
                bind_key_captures(&entry.key, k_text, *k_range, &child_ctx.path, &mut seeded);
                let sub = walk_inner(v_node, &entry.value, &child_ctx, &seeded);
                for r in sub {
                    next_templates.push(r.captures);
                }
            }
        }
        if !next_templates.is_empty() {
            row_templates = next_templates;
        }
    }

    // No descents: emit row templates directly, threaded through `rest`.
    if descents.is_empty() {
        let mut out = vec![];
        for tmpl in row_templates {
            out.extend(walk_inner(node, rest, ctx, &tmpl));
        }
        return out;
    }

    // Descents: each descent concats its results into the row output.
    // Partial semantics: a descent whose key isn't found (or whose subwalk
    // yields nothing) is skipped rather than killing the whole Object.
    // If ALL descents fail, fall back to emitting the row_templates so the
    // row_field-only captures survive.
    let mut out = vec![];
    let mut any_descent_produced = false;
    for descent in &descents {
        let key_hits = resolve_keys(node, &descent.key);
        if key_hits.is_empty() { continue; }
        for (k_text, k_range, v_node) in &key_hits {
            let child_ctx = ctx.descend_key(k_text);
            for tmpl in &row_templates {
                let mut seeded = tmpl.clone();
                bind_key_captures(&descent.key, k_text, *k_range, &child_ctx.path, &mut seeded);
                let sub = walk_inner(v_node, &descent.value, &child_ctx, &seeded);
                for r in sub {
                    any_descent_produced = true;
                    out.extend(walk_inner(node, rest, ctx, &r.captures));
                }
            }
        }
    }
    if !any_descent_produced {
        for tmpl in row_templates {
            out.extend(walk_inner(node, rest, ctx, &tmpl));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn scalar_text<N: DataNode>(n: &N) -> String {
    n.as_scalar_text().map(|c| c.into_owned()).unwrap_or_default()
}

/// Resolve an object-key matcher to the set of matching (key_text, key_byte_range, value) tuples.
fn resolve_keys<N: DataNode>(obj: &N, matcher: &CompiledKeyMatcher) -> Vec<(String, (u32, u32), N)> {
    let mut out = vec![];
    for (k, v) in obj.entries() {
        let Some(k_text) = k.as_scalar_text().map(|c| c.into_owned()) else { continue; };
        let hit = match matcher {
            CompiledKeyMatcher::Exact(name) => k_text == *name,
            CompiledKeyMatcher::Glob(matchers) => matchers.iter().any(|m| m.is_match(&k_text)),
            CompiledKeyMatcher::Capture(_) | CompiledKeyMatcher::Wildcard => true,
        };
        if hit {
            out.push((k_text, k.byte_range(), v));
        }
    }
    out
}

/// Apply key-level captures (Capture matcher or Glob with segment captures in the key).
fn bind_key_captures(
    matcher: &CompiledKeyMatcher,
    key_text: &str,
    key_range: (u32, u32),
    path: &str,
    caps: &mut Captures,
) {
    match matcher {
        CompiledKeyMatcher::Capture(name) => {
            caps.insert(
                Arc::from(name.as_str()),
                WalkCapture {
                    text: Arc::from(key_text),
                    path: Arc::from(path),
                    byte_start: key_range.0,
                    byte_end:   key_range.1,
                },
            );
        }
        CompiledKeyMatcher::Glob(matchers) => {
            for m in matchers {
                if let Some(seg_caps) = m.captures(key_text) {
                    for (name, value) in seg_caps {
                        caps.insert(
                            Arc::from(name.as_str()),
                            WalkCapture {
                                text: Arc::from(value.as_str()),
                                path: Arc::from(path),
                                byte_start: key_range.0,
                                byte_end:   key_range.1,
                            },
                        );
                    }
                }
            }
        }
        _ => {}
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{parse_by_ext, AnyDataNode};
    use crate::walk::_0_pattern::parse_segment_pattern;
    use bytes::Bytes;

    fn parse_json(src: &str) -> AnyDataNode {
        parse_by_ext("json", Arc::new(Bytes::copy_from_slice(src.as_bytes()))).unwrap()
    }

    fn key_exact(name: &str) -> CompiledKeyMatcher { CompiledKeyMatcher::Exact(name.to_string()) }
    fn leaf_cap(name: &str) -> Vec<CompiledStep> {
        vec![CompiledStep::Leaf { capture: Some(name.to_string()) }]
    }

    fn entry(key: CompiledKeyMatcher, value: Vec<CompiledStep>) -> CompiledObjectEntry {
        CompiledObjectEntry { key, value }
    }

    fn obj(entries: Vec<CompiledObjectEntry>) -> CompiledStep {
        CompiledStep::Object { entries }
    }

    fn sorted_cap_names(r: &MatchResult) -> Vec<String> {
        let mut names: Vec<_> = r.captures.keys().map(|k| k.to_string()).collect();
        names.sort();
        names
    }

    #[test]
    fn leaf_capture_binds_text_and_byte_range() {
        let src = r#"{"x": "hello"}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![entry(key_exact("x"), leaf_cap("V"))])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        let cap = &out[0].captures["V"];
        assert_eq!(&*cap.text, "hello");
        // "hello" in source is at bytes 7..14 (including quotes)
        let slice = &src.as_bytes()[cap.byte_start as usize..cap.byte_end as usize];
        assert_eq!(std::str::from_utf8(slice).unwrap(), "\"hello\"");
    }

    #[test]
    fn capture_path_is_jq_style() {
        let src = r#"{"users":[{"name":"alice"}]}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![entry(
            key_exact("users"),
            vec![CompiledStep::Array {
                item: vec![obj(vec![entry(key_exact("name"), leaf_cap("N"))])],
            }],
        )])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["N"].path, ".users[0].name");
        assert_eq!(&*out[0].captures["N"].text, "alice");
    }

    #[test]
    fn row_split_siblings_in_separate_subtrees() {
        // {a: {x: $A}, b: {y: $B}}  -> 2 rows, not 1 joined row.
        let src = r#"{"a":{"x":"foo"},"b":{"y":"bar"}}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(
                key_exact("a"),
                vec![obj(vec![entry(key_exact("x"), leaf_cap("A"))])],
            ),
            entry(
                key_exact("b"),
                vec![obj(vec![entry(key_exact("y"), leaf_cap("B"))])],
            ),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 2, "expected 2 row-split results, got {}", out.len());

        // One row binds A only, the other B only.
        let names_0 = sorted_cap_names(&out[0]);
        let names_1 = sorted_cap_names(&out[1]);
        let mut all = [names_0, names_1];
        all.sort();
        assert_eq!(all, [vec!["A".to_string()], vec!["B".to_string()]]);
    }

    #[test]
    fn object_partial_match_missing_descent_still_emits() {
        // Pattern wants both `a: {x: $A}` and `b: {y: $B}`, but input only has `a`.
        // Partial semantics: emit one row binding $A only; missing `b` entry is
        // tolerated (surfaced as a diagnostic elsewhere, not here).
        let src = r#"{"a":{"x":"foo"}}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(
                key_exact("a"),
                vec![obj(vec![entry(key_exact("x"), leaf_cap("A"))])],
            ),
            entry(
                key_exact("b"),
                vec![obj(vec![entry(key_exact("y"), leaf_cap("B"))])],
            ),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1, "partial match must emit 1 row, got {}", out.len());
        assert_eq!(&*out[0].captures["A"].text, "foo");
        assert!(!out[0].captures.contains_key("B"), "B should be unbound on partial");
    }

    #[test]
    fn object_partial_match_missing_row_field_still_emits() {
        // Row-field version: one row_field key is missing. Other row_fields
        // should still produce a row.
        let src = r#"{"name":"alice"}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(key_exact("name"), leaf_cap("N")),
            entry(key_exact("age"),  leaf_cap("AGE")),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1, "partial row_field match must emit 1 row, got {}", out.len());
        assert_eq!(&*out[0].captures["N"].text, "alice");
        assert!(!out[0].captures.contains_key("AGE"));
    }

    #[test]
    fn row_join_flat_siblings_at_same_object() {
        // {name: $N, age: $AGE}  -> 1 joined row (row_field entries).
        let src = r#"{"name":"alice","age":30}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(key_exact("name"), leaf_cap("N")),
            entry(key_exact("age"),  leaf_cap("AGE")),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["N"].text, "alice");
        assert_eq!(&*out[0].captures["AGE"].text, "30");
    }

    #[test]
    fn array_iteration_joins_row_fields_per_item() {
        // {users: [{name: $N, age: $AGE}]}  -> 1 row per user, joined.
        let src = r#"{"users":[{"name":"alice","age":30},{"name":"bob","age":25}]}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![entry(
            key_exact("users"),
            vec![CompiledStep::Array {
                item: vec![obj(vec![
                    entry(key_exact("name"), leaf_cap("N")),
                    entry(key_exact("age"),  leaf_cap("AGE")),
                ])],
            }],
        )])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 2);

        let mut pairs: Vec<_> = out.iter().map(|r| {
            (r.captures["N"].text.to_string(), r.captures["AGE"].text.to_string())
        }).collect();
        pairs.sort();
        assert_eq!(pairs, vec![
            ("alice".to_string(), "30".to_string()),
            ("bob".to_string(),   "25".to_string()),
        ]);
    }

    #[test]
    fn leaf_pattern_constrains_with_prebound_capture() {
        // Pre-seed $SCOPE="@foo", pattern "@$SCOPE/$NAME" on "@foo/bar" -> match.
        // Then on "@bar/baz" -> no match.
        let src = r#""@foo/bar""#;
        let root = parse_json(src);
        let segs = parse_segment_pattern("@$SCOPE/$NAME");
        let steps = vec![CompiledStep::LeafPattern { segments: segs }];
        let mut seed: Captures = FxHashMap::default();
        seed.insert(
            Arc::from("SCOPE"),
            WalkCapture { text: Arc::from("foo"), path: Arc::from(""), byte_start: 0, byte_end: 0 },
        );
        let out = walk_with_captures(&root, &steps, seed);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["NAME"].text, "bar");
        assert_eq!(&*out[0].captures["SCOPE"].text, "foo");
    }

    #[test]
    fn missing_row_field_tolerated_as_partial() {
        // Updated from all-or-nothing semantics: a missing key now emits a
        // row with the present captures bound and the missing one unbound.
        let src = r#"{"a":"foo"}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(key_exact("a"), leaf_cap("A")),
            entry(key_exact("missing"), leaf_cap("X")),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["A"].text, "foo");
        assert!(!out[0].captures.contains_key("X"));
    }

    #[test]
    fn mixed_row_field_and_descent() {
        // {tag: $TAG, items: [{id: $ID}]}
        // row_field TAG joins each descent row from items.
        let src = r#"{"tag":"v1","items":[{"id":"a"},{"id":"b"}]}"#;
        let root = parse_json(src);
        let steps = vec![obj(vec![
            entry(key_exact("tag"), leaf_cap("TAG")),
            entry(
                key_exact("items"),
                vec![CompiledStep::Array {
                    item: vec![obj(vec![entry(key_exact("id"), leaf_cap("ID"))])],
                }],
            ),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 2);
        let mut pairs: Vec<_> = out.iter().map(|r| (
            r.captures["TAG"].text.to_string(),
            r.captures["ID"].text.to_string(),
        )).collect();
        pairs.sort();
        assert_eq!(pairs, vec![
            ("v1".to_string(), "a".to_string()),
            ("v1".to_string(), "b".to_string()),
        ]);
    }

    #[test]
    fn key_capture_via_glob_pattern() {
        // {"@$SCOPE/$PKG": $VER}  over {"@angular/core": "1.0", "lodash": "2.0"}
        let src = r#"{"@angular/core":"1.0","lodash":"2.0"}"#;
        let root = parse_json(src);
        use crate::walk::_0_pattern::compile_pattern;
        let matchers = compile_pattern("@$SCOPE/$PKG").unwrap();
        let steps = vec![obj(vec![entry(
            CompiledKeyMatcher::Glob(matchers),
            leaf_cap("VER"),
        )])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["VER"].text, "1.0");
        assert_eq!(&*out[0].captures["SCOPE"].text, "angular");
        assert_eq!(&*out[0].captures["PKG"].text, "core");
    }

    #[test]
    fn yaml_walker_smoke() {
        // Same walker runs against YAML via AnyDataNode.
        let src = "name: alice\nage: 30\n";
        let root = parse_by_ext("yaml", Arc::new(Bytes::copy_from_slice(src.as_bytes()))).unwrap();
        let steps = vec![obj(vec![
            entry(key_exact("name"), leaf_cap("N")),
            entry(key_exact("age"),  leaf_cap("AGE")),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["N"].text, "alice");
        assert_eq!(&*out[0].captures["AGE"].text, "30");
    }

    #[test]
    fn toml_walker_smoke() {
        let src = "name = \"alice\"\nage = 30\n";
        let root = parse_by_ext("toml", Arc::new(Bytes::copy_from_slice(src.as_bytes()))).unwrap();
        let steps = vec![obj(vec![
            entry(key_exact("name"), leaf_cap("N")),
            entry(key_exact("age"),  leaf_cap("AGE")),
        ])];
        let out = walk(&root, &steps);
        assert_eq!(out.len(), 1);
        assert_eq!(&*out[0].captures["N"].text, "alice");
        assert_eq!(&*out[0].captures["AGE"].text, "30");
    }
}
