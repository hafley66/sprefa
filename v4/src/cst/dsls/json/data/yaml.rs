//! `YamlNode` — DataNode wrapping a tree-sitter-yaml parse tree.
//! Ported from v2/src/data/_2_yaml.rs (Arc<Bytes> → Arc<[u8]>).

use std::borrow::Cow;
use std::sync::Arc;

use tree_sitter::{Node, Parser, Tree};

use super::types::{DataKind, DataNode, ParseError};

struct Inner {
    src:  Arc<[u8]>,
    tree: Tree,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

#[derive(Clone)]
pub struct YamlNode {
    inner:   Arc<Inner>,
    node_id: usize,
}

impl YamlNode {
    pub fn parse(src: Arc<[u8]>) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_yaml::LANGUAGE.into())
            .map_err(|e| ParseError::new(Arc::from(format!("language error: {e}"))))?;

        let tree = parser
            .parse(src.as_ref(), None)
            .ok_or_else(|| ParseError::new("tree-sitter-yaml: parse returned None"))?;

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
    fn kind(&self) -> DataKind { yaml_kind(self.node()) }

    fn byte_range(&self) -> (u32, u32) {
        let n = self.node();
        (n.start_byte() as u32, n.end_byte() as u32)
    }

    fn as_scalar_text(&self) -> Option<Cow<'_, str>> {
        yaml_scalar_text(self.node(), &self.inner.src)
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_> {
        Box::new(collect_mapping_pairs(self.node(), self).into_iter())
    }

    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_> {
        Box::new(collect_sequence_items(self.node(), self).into_iter())
    }

    fn source(&self) -> &[u8] { &self.inner.src }
}

fn yaml_kind(n: Node<'_>) -> DataKind {
    match n.kind() {
        "block_mapping" | "flow_mapping"   => DataKind::Object,
        "block_sequence" | "flow_sequence" => DataKind::Array,
        "null_scalar"                      => DataKind::Null,
        "plain_scalar" => {
            if let Some(child) = n.named_child(0) {
                if child.kind() == "null_scalar" { return DataKind::Null; }
            }
            DataKind::Scalar
        }
        _ => DataKind::Scalar,
    }
}

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
        "plain_scalar" => yaml_scalar_text(n.named_child(0)?, src),
        "null_scalar"  => None,
        "block_scalar" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(raw).into_owned()))
        }
        _ => {
            let raw = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(raw).into_owned()))
        }
    }
}

fn unescape_yaml_double_quoted(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n')  => out.push('\n'),
                Some('t')  => out.push('\t'),
                Some('r')  => out.push('\r'),
                Some('"')  => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('0')  => out.push('\0'),
                Some('a')  => out.push('\x07'),
                Some('b')  => out.push('\x08'),
                Some('f')  => out.push('\x0C'),
                Some('v')  => out.push('\x0B'),
                Some('e')  => out.push('\x1B'),
                Some('N')  => out.push('\u{85}'),
                Some('_')  => out.push('\u{A0}'),
                Some('L')  => out.push('\u{2028}'),
                Some('P')  => out.push('\u{2029}'),
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
        if let Some(found) = find_node_by_id(child, id) { return Some(found); }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> YamlNode {
        YamlNode::parse(Arc::from(s.as_bytes())).unwrap()
    }

    #[test]
    fn block_mapping_and_byte_ranges() {
        let src = "a: 1\nb: hello\n";
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
        let (k0, v0) = &entries[0];
        assert_eq!(k0.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(v0.as_scalar_text().unwrap().as_ref(), "1");
        let (ks, ke) = k0.byte_range();
        assert_eq!(&src[ks as usize..ke as usize], "a");
    }

    #[test]
    fn nested_and_array() {
        let src = "top:\n  inner:\n    - one\n    - two\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, inner) = &entries[0];
        let inner_entries: Vec<_> = inner.entries().collect();
        let (_, arr) = &inner_entries[0];
        assert_eq!(arr.kind(), DataKind::Array);
        let items: Vec<_> = arr.items().collect();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].as_scalar_text().unwrap().as_ref(), "two");
    }

    #[test]
    fn yaml_bools_null() {
        let src = "t: true\nf: false\nn: null\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries[2].1.kind(), DataKind::Null);
        assert!(entries[2].1.as_scalar_text().is_none());
    }
}
