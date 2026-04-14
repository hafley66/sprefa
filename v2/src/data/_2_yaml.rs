use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use tree_sitter::{Node, Parser, Tree};

use super::_0_types::{DataKind, DataNode, ParseError};

// ---------------------------------------------------------------------------
// Inner
// ---------------------------------------------------------------------------

struct Inner {
    src: Arc<Bytes>,
    tree: Tree,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

// ---------------------------------------------------------------------------
// YamlNode
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct YamlNode {
    inner: Arc<Inner>,
    node_id: usize,
}

impl YamlNode {
    pub fn parse(src: Arc<Bytes>) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_yaml::LANGUAGE.into())
            .map_err(|e| ParseError::new(Arc::from(format!("language error: {e}"))))?;

        let tree = parser
            .parse(src.as_ref(), None)
            .ok_or_else(|| ParseError::new("tree-sitter-yaml: parse returned None"))?;

        // stream -> document -> block_node/flow_node -> actual value node
        let root = tree.root_node();
        let value_node = dig_to_value(root).unwrap_or(root);
        let node_id = value_node.id();
        Ok(YamlNode { inner: Arc::new(Inner { src, tree }), node_id })
    }

    fn node(&self) -> Node<'_> {
        find_node_by_id(self.inner.tree.root_node(), self.node_id)
            .expect("yaml node id not found; this is a bug")
    }

    fn wrap(&self, n: Node<'_>) -> YamlNode {
        YamlNode { inner: self.inner.clone(), node_id: n.id() }
    }
}

impl DataNode for YamlNode {
    fn kind(&self) -> DataKind {
        yaml_kind(self.node())
    }

    fn byte_range(&self) -> (u32, u32) {
        let n = self.node();
        (n.start_byte() as u32, n.end_byte() as u32)
    }

    fn as_scalar_text(&self) -> Option<Cow<'_, str>> {
        let n = self.node();
        let src = self.inner.src.as_ref();
        yaml_scalar_text(n, src)
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_> {
        let n = self.node();
        let pairs = collect_mapping_pairs(n, self);
        Box::new(pairs.into_iter())
    }

    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_> {
        let n = self.node();
        let items = collect_sequence_items(n, self);
        Box::new(items.into_iter())
    }

    fn source(&self) -> &[u8] {
        &self.inner.src
    }
}

// ---------------------------------------------------------------------------
// Kind classification
// ---------------------------------------------------------------------------

fn yaml_kind(n: Node<'_>) -> DataKind {
    match n.kind() {
        "block_mapping" | "flow_mapping" => DataKind::Object,
        "block_sequence" | "flow_sequence" => DataKind::Array,
        "null_scalar" => DataKind::Null,
        "plain_scalar" => {
            if let Some(child) = n.named_child(0) {
                if child.kind() == "null_scalar" { return DataKind::Null; }
            }
            DataKind::Scalar
        }
        _ => DataKind::Scalar,
    }
}

// ---------------------------------------------------------------------------
// Scalar text extraction
// ---------------------------------------------------------------------------

fn yaml_scalar_text<'a>(n: Node<'_>, src: &'a [u8]) -> Option<Cow<'a, str>> {
    match n.kind() {
        "double_quote_scalar" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            let s = std::str::from_utf8(raw).ok()?;
            let inner = &s[1..s.len() - 1];
            Some(Cow::Owned(unescape_yaml_double_quoted(inner)))
        }
        "single_quote_scalar" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            let s = std::str::from_utf8(raw).ok()?;
            let inner = &s[1..s.len() - 1];
            Some(Cow::Owned(inner.replace("''", "'")))
        }
        "plain_scalar" => {
            let child = n.named_child(0)?;
            yaml_scalar_text(child, src)
        }
        "null_scalar" => None,
        "block_scalar" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(raw).into_owned()))
        }
        _ => {
            // boolean_scalar, integer_scalar, float_scalar, string_scalar, timestamp_scalar
            let raw = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(raw).into_owned()))
        }
    }
}

// YAML double-quoted escaping is a superset of JSON: same sequences plus
// \e, \N, \_, \L, \P, and multi-byte \u/\U.
fn unescape_yaml_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('0') => out.push('\0'),
                Some('a') => out.push('\x07'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0C'),
                Some('v') => out.push('\x0B'),
                Some('e') => out.push('\x1B'),
                Some('N') => out.push('\u{85}'),
                Some('_') => out.push('\u{A0}'),
                Some('L') => out.push('\u{2028}'),
                Some('P') => out.push('\u{2029}'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) { out.push(ch); }
                    }
                }
                Some('U') => {
                    let hex: String = chars.by_ref().take(8).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) { out.push(ch); }
                    }
                }
                Some(c) => { out.push('\\'); out.push(c); }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Navigation helpers
// ---------------------------------------------------------------------------

/// Walk stream -> document -> block_node/flow_node -> mapping/sequence/scalar.
fn dig_to_value(n: Node<'_>) -> Option<Node<'_>> {
    match n.kind() {
        "stream" => {
            let doc = (0..n.named_child_count())
                .find_map(|i| n.named_child(i).filter(|c| c.kind() == "document"))?;
            dig_to_value(doc)
        }
        "document" => {
            let content = (0..n.named_child_count()).find_map(|i| {
                let c = n.named_child(i)?;
                match c.kind() {
                    "block_node" | "flow_node" => Some(c),
                    _ => None,
                }
            })?;
            dig_to_value(content)
        }
        "block_node" | "flow_node" => {
            (0..n.named_child_count()).find_map(|i| {
                let c = n.named_child(i)?;
                match c.kind() {
                    "anchor" | "tag" => None,
                    _ => Some(c),
                }
            })
        }
        _ => Some(n),
    }
}

/// Unwrap block_node/flow_node to their inner value node.
fn unwrap_node(n: Node<'_>) -> Node<'_> {
    match n.kind() {
        "block_node" | "flow_node" => {
            for i in 0..n.named_child_count() {
                if let Some(c) = n.named_child(i) {
                    match c.kind() {
                        "anchor" | "tag" => continue,
                        _ => return c,
                    }
                }
            }
            n
        }
        _ => n,
    }
}

fn collect_mapping_pairs(n: Node<'_>, owner: &YamlNode) -> Vec<(YamlNode, YamlNode)> {
    let mut pairs = Vec::new();
    for i in 0..n.named_child_count() {
        let child = match n.named_child(i) { Some(c) => c, None => continue };
        match child.kind() {
            "block_mapping_pair" | "flow_pair" => {
                let key_raw = match child.child_by_field_name("key") {
                    Some(k) => unwrap_node(k),
                    None => continue,
                };
                let val_raw = match child.child_by_field_name("value") {
                    Some(v) => unwrap_node(v),
                    None => continue,
                };
                pairs.push((owner.wrap(key_raw), owner.wrap(val_raw)));
            }
            _ => {}
        }
    }
    pairs
}

fn collect_sequence_items(n: Node<'_>, owner: &YamlNode) -> Vec<YamlNode> {
    let mut items = Vec::new();
    for i in 0..n.named_child_count() {
        let child = match n.named_child(i) { Some(c) => c, None => continue };
        match child.kind() {
            "block_sequence_item" => {
                if let Some(inner) = child.named_child(0) {
                    items.push(owner.wrap(unwrap_node(inner)));
                }
            }
            "flow_node" | "flow_pair" => {
                items.push(owner.wrap(unwrap_node(child)));
            }
            _ => {}
        }
    }
    items
}

fn find_node_by_id<'a>(root: Node<'a>, id: usize) -> Option<Node<'a>> {
    if root.id() == id { return Some(root); }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(found) = find_node_by_id(child, id) {
            return Some(found);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> YamlNode {
        YamlNode::parse(Arc::new(Bytes::from(s.to_owned()))).unwrap()
    }

    #[test]
    fn block_mapping() {
        let src = "a: 1\nb: hello\n";
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
        let (k0, v0) = &entries[0];
        assert_eq!(k0.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(v0.as_scalar_text().unwrap().as_ref(), "1");
        let (k1, v1) = &entries[1];
        assert_eq!(k1.as_scalar_text().unwrap().as_ref(), "b");
        assert_eq!(v1.as_scalar_text().unwrap().as_ref(), "hello");
    }

    #[test]
    fn block_mapping_byte_ranges() {
        let src = "a: 1\nb: 2\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (k0, v0) = &entries[0];
        let (ks, ke) = k0.byte_range();
        assert_eq!(&src[ks as usize..ke as usize], "a");
        let (vs, ve) = v0.byte_range();
        assert_eq!(&src[vs as usize..ve as usize], "1");
        let (k1, _) = &entries[1];
        let (k1s, k1e) = k1.byte_range();
        assert_eq!(&src[k1s as usize..k1e as usize], "b");
    }

    #[test]
    fn flow_mapping() {
        let src = "{x: 10, y: 20}";
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn nested_and_array() {
        let src = "top:\n  inner:\n    - one\n    - two\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, inner) = &entries[0];
        assert_eq!(inner.kind(), DataKind::Object);
        let inner_entries: Vec<_> = inner.entries().collect();
        let (_, arr) = &inner_entries[0];
        assert_eq!(arr.kind(), DataKind::Array);
        let items: Vec<_> = arr.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].as_scalar_text().unwrap().as_ref(), "one");
        assert_eq!(items[1].as_scalar_text().unwrap().as_ref(), "two");
    }

    #[test]
    fn yaml_comments_ignored() {
        let src = "# comment\na: 1 # inline\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 1);
        let (k, v) = &entries[0];
        assert_eq!(k.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(v.as_scalar_text().unwrap().as_ref(), "1");
    }

    #[test]
    fn yaml_bools_null() {
        let src = "t: true\nf: false\nn: null\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, v0) = &entries[0];
        let (_, v1) = &entries[1];
        let (_, v2) = &entries[2];
        assert_eq!(v0.kind(), DataKind::Scalar);
        assert_eq!(v0.as_scalar_text().unwrap().as_ref(), "true");
        assert_eq!(v1.as_scalar_text().unwrap().as_ref(), "false");
        assert_eq!(v2.kind(), DataKind::Null);
        assert!(v2.as_scalar_text().is_none());
    }

    #[test]
    fn double_quoted_escape() {
        let src = "s: \"a\\nb\"\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, v) = &entries[0];
        assert_eq!(v.as_scalar_text().unwrap().as_ref(), "a\nb");
    }
}
