//! `JsonNode` — DataNode wrapping a tree-sitter-json parse tree.
//! Ported from v2/src/data/_1_json.rs. Source bytes are `Arc<[u8]>` to
//! match v3 Cursor.content.

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
pub struct JsonNode {
    inner:   Arc<Inner>,
    node_id: usize,
}

impl JsonNode {
    pub fn parse(src: Arc<[u8]>) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .map_err(|e| ParseError::new(Arc::from(format!("language error: {e}"))))?;

        let tree = parser
            .parse(src.as_ref(), None)
            .ok_or_else(|| ParseError::new("tree-sitter-json: parse returned None"))?;

        let root = tree.root_node();
        let value_node = first_named_child(root).unwrap_or(root);
        let node_id = value_node.id();
        Ok(JsonNode { inner: Arc::new(Inner { src, tree }), node_id })
    }

    fn node(&self) -> Node<'_> {
        find_node_by_id(self.inner.tree.root_node(), self.node_id)
            .expect("json node id not found; this is a bug")
    }

    fn wrap(&self, n: Node<'_>) -> JsonNode {
        JsonNode { inner: self.inner.clone(), node_id: n.id() }
    }
}

impl DataNode for JsonNode {
    fn kind(&self) -> DataKind { json_kind(self.node()) }

    fn byte_range(&self) -> (u32, u32) {
        let n = self.node();
        (n.start_byte() as u32, n.end_byte() as u32)
    }

    fn as_scalar_text(&self) -> Option<Cow<'_, str>> {
        let n = self.node();
        let src = self.inner.src.as_ref();
        json_scalar_text(n, src)
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_> {
        let n = self.node();
        let pairs: Vec<_> = (0..n.named_child_count())
            .filter_map(|i| {
                let child = n.named_child(i)?;
                if child.kind() != "pair" { return None; }
                let key = child.child_by_field_name("key")?;
                let val = child.child_by_field_name("value")?;
                Some((self.wrap(key), self.wrap(val)))
            })
            .collect();
        Box::new(pairs.into_iter())
    }

    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_> {
        let n = self.node();
        let items: Vec<_> = (0..n.named_child_count())
            .filter_map(|i| Some(self.wrap(n.named_child(i)?)))
            .collect();
        Box::new(items.into_iter())
    }

    fn source(&self) -> &[u8] { &self.inner.src }
}

fn json_kind(n: Node<'_>) -> DataKind {
    match n.kind() {
        "object" => DataKind::Object,
        "array"  => DataKind::Array,
        "null"   => DataKind::Null,
        _        => DataKind::Scalar,
    }
}

fn json_scalar_text<'a>(n: Node<'_>, src: &'a [u8]) -> Option<Cow<'a, str>> {
    match n.kind() {
        "string" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            if raw.len() < 2 { return None; }
            let inner = &raw[1..raw.len() - 1];
            let s = std::str::from_utf8(inner).ok()?;
            Some(Cow::Owned(unescape_json_string(s)))
        }
        "number" | "true" | "false" => {
            let bytes = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(bytes).into_owned()))
        }
        _ => None,
    }
}

fn unescape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"')  => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/')  => out.push('/'),
                Some('b')  => out.push('\x08'),
                Some('f')  => out.push('\x0C'),
                Some('n')  => out.push('\n'),
                Some('r')  => out.push('\r'),
                Some('t')  => out.push('\t'),
                Some('u')  => {
                    let hex: String = chars.by_ref().take(4).collect();
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

fn first_named_child(n: Node<'_>) -> Option<Node<'_>> {
    (0..n.named_child_count()).find_map(|i| n.named_child(i))
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

    fn parse(s: &str) -> JsonNode {
        JsonNode::parse(Arc::from(s.as_bytes())).unwrap()
    }

    #[test]
    fn object_byte_ranges_and_entries() {
        let src = r#"{"a":1,"b":"hi"}"#;
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(entries[0].1.as_scalar_text().unwrap().as_ref(), "1");
        assert_eq!(entries[1].1.as_scalar_text().unwrap().as_ref(), "hi");
    }

    #[test]
    fn nested_array_items() {
        let src = r#"{"x":[1,2,null]}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let arr = &entries[0].1;
        assert_eq!(arr.kind(), DataKind::Array);
        let items: Vec<_> = arr.items().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].kind(), DataKind::Null);
    }

    #[test]
    fn string_escape() {
        let src = r#"{"s":"a\nb"}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries[0].1.as_scalar_text().unwrap().as_ref(), "a\nb");
    }
}
