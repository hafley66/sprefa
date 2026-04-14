use std::borrow::Cow;
use std::sync::Arc;

use bytes::Bytes;
use tree_sitter::{Node, Parser, Tree};

use super::_0_types::{DataKind, DataNode, ParseError};

// ---------------------------------------------------------------------------
// Inner — shared across all clones of a JsonNode from the same parse
// ---------------------------------------------------------------------------

struct Inner {
    src: Arc<Bytes>,
    tree: Tree,
}

// SAFETY: Tree holds immutable parsed data after parse(); no mutable state
// is accessed concurrently.
unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

// ---------------------------------------------------------------------------
// JsonNode
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct JsonNode {
    inner: Arc<Inner>,
    // Node borrows from Tree so we can't store it directly. Store the node id
    // and re-derive on every access via DFS.
    node_id: usize,
}

impl JsonNode {
    pub fn parse(src: Arc<Bytes>) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_json::LANGUAGE.into())
            .map_err(|e| ParseError::new(Arc::from(format!("language error: {e}"))))?;

        let tree = parser
            .parse(src.as_ref(), None)
            .ok_or_else(|| ParseError::new("tree-sitter-json: parse returned None"))?;

        // document root -> first meaningful child (the actual value)
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
    fn kind(&self) -> DataKind {
        json_kind(self.node())
    }

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

    fn source(&self) -> &[u8] {
        &self.inner.src
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_kind(n: Node<'_>) -> DataKind {
    match n.kind() {
        "object" => DataKind::Object,
        "array" => DataKind::Array,
        "null" => DataKind::Null,
        _ => DataKind::Scalar,
    }
}

fn json_scalar_text<'a>(n: Node<'_>, src: &'a [u8]) -> Option<Cow<'a, str>> {
    match n.kind() {
        "string" => {
            let raw = &src[n.start_byte()..n.end_byte()];
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
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('/') => out.push('/'),
                Some('b') => out.push('\x08'),
                Some('f') => out.push('\x0C'),
                Some('n') => out.push('\n'),
                Some('r') => out.push('\r'),
                Some('t') => out.push('\t'),
                Some('u') => {
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

    fn parse(s: &str) -> JsonNode {
        JsonNode::parse(Arc::new(Bytes::from(s.to_owned()))).unwrap()
    }

    #[test]
    fn object_byte_ranges() {
        let src = r#"{"a":1,"b":"hi"}"#;
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let (s, e) = root.byte_range();
        assert_eq!(&src[s as usize..e as usize], src);

        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);

        let (k0, v0) = &entries[0];
        let (ks, ke) = k0.byte_range();
        assert_eq!(&src[ks as usize..ke as usize], r#""a""#);
        assert_eq!(k0.as_scalar_text().unwrap().as_ref(), "a");

        let (vs, ve) = v0.byte_range();
        assert_eq!(&src[vs as usize..ve as usize], "1");
        assert_eq!(v0.as_scalar_text().unwrap().as_ref(), "1");

        let (k1, v1) = &entries[1];
        assert_eq!(k1.as_scalar_text().unwrap().as_ref(), "b");
        assert_eq!(v1.as_scalar_text().unwrap().as_ref(), "hi");
    }

    #[test]
    fn nested_object_and_array() {
        let src = r#"{"x":{"arr":[1,true,null]}}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 1);
        let (_, inner) = &entries[0];
        assert_eq!(inner.kind(), DataKind::Object);
        let inner_entries: Vec<_> = inner.entries().collect();
        let (_, arr) = &inner_entries[0];
        assert_eq!(arr.kind(), DataKind::Array);
        let items: Vec<_> = arr.items().collect();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].as_scalar_text().unwrap().as_ref(), "1");
        assert_eq!(items[1].as_scalar_text().unwrap().as_ref(), "true");
        assert_eq!(items[2].kind(), DataKind::Null);
        assert!(items[2].as_scalar_text().is_none());
    }

    #[test]
    fn repeated_keys_both_visible() {
        let src = r#"{"a":1,"a":2}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
        let (k0, v0) = &entries[0];
        let (k1, v1) = &entries[1];
        assert_eq!(k0.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(v0.as_scalar_text().unwrap().as_ref(), "1");
        assert_eq!(k1.as_scalar_text().unwrap().as_ref(), "a");
        assert_eq!(v1.as_scalar_text().unwrap().as_ref(), "2");
        assert_ne!(k0.byte_range(), k1.byte_range());
        assert_ne!(v0.byte_range(), v1.byte_range());
    }

    #[test]
    fn string_escape() {
        let src = r#"{"s":"a\nb"}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, v) = &entries[0];
        assert_eq!(v.as_scalar_text().unwrap().as_ref(), "a\nb");
    }

    #[test]
    fn scalars_bools_null() {
        let src = r#"{"t":true,"f":false,"n":null,"num":42}"#;
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let vals: Vec<_> = entries.iter().map(|(_, v)| v).collect();
        assert_eq!(vals[0].kind(), DataKind::Scalar);
        assert_eq!(vals[0].as_scalar_text().unwrap().as_ref(), "true");
        assert_eq!(vals[1].as_scalar_text().unwrap().as_ref(), "false");
        assert_eq!(vals[2].kind(), DataKind::Null);
        assert!(vals[2].as_scalar_text().is_none());
        assert_eq!(vals[3].as_scalar_text().unwrap().as_ref(), "42");
    }
}
