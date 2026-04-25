//! Walker: evaluate `Vec<CompiledStep>` against a `DataNode` tree.
//! Ported from v2/src/walk/_3_walker.rs. v3 strips the per-capture
//! `path` field; bytes are tracked by `(byte_start, byte_end)` only.
//!
//! Object entries split into `row_field` (one Leaf/LeafPattern/CaptureAny
//! at the immediate value) vs descents (anything else). Row fields join
//! into one row per match; descents fan out into separate rows.

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::compiled::{
    Captures, CompiledKeyMatcher, CompiledObjectEntry, CompiledStep, WalkCapture,
};
use super::pattern::match_segments_with_bindings;
use crate::data::{DataKind, DataNode};

#[derive(Debug, Clone)]
pub struct MatchResult {
    pub captures: Captures,
}

#[derive(Debug, Clone)]
pub struct WalkOutcome {
    pub rows:        Vec<MatchResult>,
    pub missed_keys: Vec<Arc<str>>,
}

#[derive(Clone)]
struct Ctx {
    depth:      u32,
    parent_key: Option<Arc<str>>,
}

impl Ctx {
    fn new() -> Self { Self { depth: 0, parent_key: None } }
    fn descend_key(&self, key: &str) -> Self {
        Self { depth: self.depth + 1, parent_key: Some(Arc::from(key)) }
    }
    fn descend_index(&self) -> Self {
        Self { depth: self.depth + 1, parent_key: None }
    }
}

pub fn walk<N: DataNode>(root: &N, steps: &[CompiledStep]) -> WalkOutcome {
    walk_with_captures(root, steps, FxHashMap::default())
}

pub fn walk_with_captures<N: DataNode>(
    root:  &N,
    steps: &[CompiledStep],
    seed:  Captures,
) -> WalkOutcome {
    let missed_keys = collect_top_level_missed_keys(root, steps);
    let rows = walk_inner(root, steps, &Ctx::new(), &seed);
    WalkOutcome { rows, missed_keys }
}

fn collect_top_level_missed_keys<N: DataNode>(
    root: &N,
    steps: &[CompiledStep],
) -> Vec<Arc<str>> {
    let Some(CompiledStep::Object { entries }) = steps.first() else { return vec![]; };
    if root.kind() != DataKind::Object { return vec![]; }
    let mut missed = vec![];
    for entry in entries {
        match &entry.key {
            CompiledKeyMatcher::Exact(name) => {
                if resolve_keys(root, &entry.key).is_empty() {
                    missed.push(Arc::from(name.as_str()));
                }
            }
            CompiledKeyMatcher::Glob(_) => {
                if resolve_keys(root, &entry.key).is_empty() {
                    missed.push(Arc::from("<glob>"));
                }
            }
            _ => {}
        }
    }
    missed
}

fn walk_inner<N: DataNode>(
    node:  &N,
    steps: &[CompiledStep],
    ctx:   &Ctx,
    caps:  &Captures,
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
                    for v in node.items() {
                        let child_ctx = ctx.descend_index();
                        out.extend(walk_inner(&v, steps, &child_ctx, caps));
                    }
                }
                _ => {}
            }
            out
        }
        CompiledStep::Key { name, capture } => {
            if node.kind() != DataKind::Object { return vec![]; }
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
                                text:       Arc::from(k_text),
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
            if node.kind() != DataKind::Object { return vec![]; }
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
                                text:       Arc::from(k_text.as_str()),
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
            Some(pk) if matchers.iter().any(|m| m.is_match(pk)) => walk_inner(node, rest, ctx, caps),
            _ => vec![],
        },
        CompiledStep::ArrayItem => {
            if node.kind() != DataKind::Array { return vec![]; }
            let mut out = vec![];
            for v in node.items() {
                let child_ctx = ctx.descend_index();
                out.extend(walk_inner(&v, rest, &child_ctx, caps));
            }
            out
        }
        CompiledStep::Leaf { capture } => {
            let Some(text) = node.as_scalar_text() else { return vec![]; };
            let mut next_caps = caps.clone();
            if let Some(cap) = capture {
                if let Some(existing) = caps.get(cap.as_str()) {
                    if existing.text.as_ref() != text.as_ref() { return vec![]; }
                } else {
                    let (bs, be) = node.byte_range();
                    next_caps.insert(
                        Arc::from(cap.as_str()),
                        WalkCapture {
                            text:       Arc::from(text.as_ref()),
                            byte_start: bs,
                            byte_end:   be,
                        },
                    );
                }
            }
            walk_inner(node, rest, ctx, &next_caps)
        }
        CompiledStep::CaptureAny { capture } => {
            let (bs, be) = node.byte_range();
            let text: Arc<str> = node
                .as_scalar_text()
                .map(|t| Arc::from(t.as_ref()))
                .unwrap_or_else(|| Arc::from(""));
            let mut next_caps = caps.clone();
            if let Some(cap) = capture {
                if let Some(existing) = caps.get(cap.as_str()) {
                    if node.as_scalar_text().is_some() && existing.text.as_ref() != text.as_ref() {
                        return vec![];
                    }
                } else {
                    next_caps.insert(
                        Arc::from(cap.as_str()),
                        WalkCapture { text, byte_start: bs, byte_end: be },
                    );
                }
            }
            walk_inner(node, rest, ctx, &next_caps)
        }
        CompiledStep::LeafPattern { segments } => {
            let Some(text) = node.as_scalar_text() else { return vec![]; };
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
                if !caps.contains_key(name.as_str()) {
                    next_caps.insert(
                        Arc::from(name.as_str()),
                        WalkCapture {
                            text:       Arc::from(value.as_str()),
                            byte_start: bs,
                            byte_end:   be,
                        },
                    );
                }
            }
            walk_inner(node, rest, ctx, &next_caps)
        }
        CompiledStep::Object { entries } => {
            if node.kind() != DataKind::Object { return vec![]; }
            walk_object_entries(node, entries, rest, ctx, caps)
        }
        CompiledStep::Array { item } => {
            if node.kind() != DataKind::Array { return vec![]; }
            let mut out = vec![];
            for v in node.items() {
                let child_ctx = ctx.descend_index();
                let sub = walk_inner(&v, item, &child_ctx, caps);
                for r in sub {
                    out.extend(walk_inner(node, rest, ctx, &r.captures));
                }
            }
            out
        }
    }
}

fn is_row_field(entry: &CompiledObjectEntry) -> bool {
    entry.value.len() == 1
        && matches!(
            entry.value[0],
            CompiledStep::Leaf { .. }
                | CompiledStep::LeafPattern { .. }
                | CompiledStep::CaptureAny { .. }
        )
}

fn walk_object_entries<N: DataNode>(
    node:    &N,
    entries: &[CompiledObjectEntry],
    rest:    &[CompiledStep],
    ctx:     &Ctx,
    caps:    &Captures,
) -> Vec<MatchResult> {
    let mut row_fields: Vec<&CompiledObjectEntry> = vec![];
    let mut descents:   Vec<&CompiledObjectEntry> = vec![];
    for e in entries {
        if is_row_field(e) { row_fields.push(e); } else { descents.push(e); }
    }

    let mut row_templates: Vec<Captures> = vec![caps.clone()];
    for entry in &row_fields {
        let key_hits = resolve_keys(node, &entry.key);
        if key_hits.is_empty() { continue; }
        let mut next_templates = vec![];
        for tmpl in &row_templates {
            for (k_text, k_range, v_node) in &key_hits {
                let child_ctx = ctx.descend_key(k_text);
                let mut seeded = tmpl.clone();
                bind_key_captures(&entry.key, k_text, *k_range, &mut seeded);
                let sub = walk_inner(v_node, &entry.value, &child_ctx, &seeded);
                for r in sub { next_templates.push(r.captures); }
            }
        }
        if !next_templates.is_empty() { row_templates = next_templates; }
    }

    if descents.is_empty() {
        let mut out = vec![];
        for tmpl in row_templates {
            out.extend(walk_inner(node, rest, ctx, &tmpl));
        }
        return out;
    }

    let mut out = vec![];
    let mut any_descent_produced = false;
    for descent in &descents {
        let key_hits = resolve_keys(node, &descent.key);
        if key_hits.is_empty() { continue; }
        for (k_text, k_range, v_node) in &key_hits {
            let child_ctx = ctx.descend_key(k_text);
            for tmpl in &row_templates {
                let mut seeded = tmpl.clone();
                bind_key_captures(&descent.key, k_text, *k_range, &mut seeded);
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

fn scalar_text<N: DataNode>(n: &N) -> String {
    n.as_scalar_text().map(|c| c.into_owned()).unwrap_or_default()
}

fn resolve_keys<N: DataNode>(
    obj: &N,
    matcher: &CompiledKeyMatcher,
) -> Vec<(String, (u32, u32), N)> {
    let mut out = vec![];
    for (k, v) in obj.entries() {
        let Some(k_text) = k.as_scalar_text().map(|c| c.into_owned()) else { continue; };
        let hit = match matcher {
            CompiledKeyMatcher::Exact(name) => k_text == *name,
            CompiledKeyMatcher::Glob(matchers) => matchers.iter().any(|m| m.is_match(&k_text)),
            CompiledKeyMatcher::Capture(_) | CompiledKeyMatcher::Wildcard => true,
        };
        if hit { out.push((k_text, k.byte_range(), v)); }
    }
    out
}

fn bind_key_captures(
    matcher:  &CompiledKeyMatcher,
    key_text: &str,
    key_range: (u32, u32),
    caps:     &mut Captures,
) {
    match matcher {
        CompiledKeyMatcher::Capture(name) => {
            caps.insert(
                Arc::from(name.as_str()),
                WalkCapture {
                    text:       Arc::from(key_text),
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
                                text:       Arc::from(value.as_str()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::brace_parse::parse_body;
    use super::super::compile::compile_steps;
    use crate::data::JsonNode;

    fn parse_json(src: &str) -> JsonNode {
        JsonNode::parse(Arc::from(src.as_bytes())).unwrap()
    }

    fn compile(body: &str) -> Vec<CompiledStep> {
        let (steps, _) = parse_body(body).unwrap();
        compile_steps(&steps).unwrap()
    }

    #[test]
    fn flat_object_captures() {
        let json = parse_json(r#"{"name":"alice","age":30}"#);
        let steps = compile("{ name: $N, age: $AGE }");
        let out = walk(&json, &steps);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(&*out.rows[0].captures["N"].text, "alice");
        assert_eq!(&*out.rows[0].captures["AGE"].text, "30");
    }

    #[test]
    fn array_iter_joins_per_item() {
        let json = parse_json(r#"{"users":[{"name":"a","age":1},{"name":"b","age":2}]}"#);
        let steps = compile("{ users: [...{ name: $N, age: $A }] }");
        let out = walk(&json, &steps);
        assert_eq!(out.rows.len(), 2);
        let mut pairs: Vec<_> = out.rows.iter()
            .map(|r| (r.captures["N"].text.to_string(), r.captures["A"].text.to_string()))
            .collect();
        pairs.sort();
        assert_eq!(pairs, vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
    }

    #[test]
    fn captured_key() {
        let json = parse_json(r#"{"deps":{"foo":"1.0","bar":"2.0"}}"#);
        let steps = compile("{ deps: { $K: $V } }");
        let out = walk(&json, &steps);
        assert_eq!(out.rows.len(), 2);
        let mut pairs: Vec<_> = out.rows.iter()
            .map(|r| (r.captures["K"].text.to_string(), r.captures["V"].text.to_string()))
            .collect();
        pairs.sort();
        assert_eq!(pairs, vec![("bar".into(), "2.0".into()), ("foo".into(), "1.0".into())]);
    }

    #[test]
    fn double_star_descends() {
        let json = parse_json(r#"{"a":{"b":{"image":"nginx"}}}"#);
        let steps = compile("{ **: { image: $I } }");
        let out = walk(&json, &steps);
        // ** is recursive; should reach the image at any depth.
        let imgs: Vec<_> = out.rows.iter()
            .filter_map(|r| r.captures.get("I").map(|c| c.text.to_string()))
            .collect();
        assert!(imgs.contains(&"nginx".to_string()));
    }

    #[test]
    fn leaf_pattern_splits() {
        let json = parse_json(r#"{"image":"nginx:1.21"}"#);
        let steps = compile(r#"{ image: "$REPO:$TAG" }"#);
        let out = walk(&json, &steps);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(&*out.rows[0].captures["REPO"].text, "nginx");
        assert_eq!(&*out.rows[0].captures["TAG"].text, "1.21");
    }

    #[test]
    fn missed_keys_surface() {
        let json = parse_json(r#"{"a":"foo"}"#);
        let steps = compile("{ a: $A, missing: $X }");
        let out = walk(&json, &steps);
        assert_eq!(out.rows.len(), 1);
        assert!(!out.rows[0].captures.contains_key("X"));
        assert!(out.missed_keys.iter().any(|k| k.as_ref() == "missing"));
    }
}
