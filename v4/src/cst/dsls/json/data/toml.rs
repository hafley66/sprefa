//! `TomlNode` — DataNode wrapping a tree-sitter-toml-ng parse tree.
//! Ported from v2/src/data/_3_toml.rs (Arc<Bytes> → Arc<[u8]>).
//!
//! TOML's CST is flat: [table] / [[array_of_tables]] sections sit as
//! siblings of root-level pairs inside `document`. The document root
//! exposes both bare pairs and table headers as its entries; each
//! `[[fruits]]` section yields a distinct entry keyed `fruits`. True
//! grouping into a single Array entry needs walker-layer support.

use std::borrow::Cow;
use std::sync::Arc;

use tree_sitter::{Node, Parser, Tree};

use super::types::{DataKind, DataNode, ParseError};

struct Inner {
    src: Arc<[u8]>,
    tree: Tree,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

#[derive(Clone)]
pub struct TomlNode {
    inner: Arc<Inner>,
    node_id: usize,
    is_document_root: bool,
}

impl TomlNode {
    pub fn parse(src: Arc<[u8]>) -> Result<Self, ParseError> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_toml_ng::LANGUAGE.into())
            .map_err(|e| ParseError::new(Arc::from(format!("language error: {e}"))))?;

        let tree = parser
            .parse(src.as_ref(), None)
            .ok_or_else(|| ParseError::new("tree-sitter-toml-ng: parse returned None"))?;

        let root_id = tree.root_node().id();
        Ok(TomlNode {
            inner: Arc::new(Inner { src, tree }),
            node_id: root_id,
            is_document_root: true,
        })
    }

    fn node(&self) -> Node<'_> {
        find_node_by_id(self.inner.tree.root_node(), self.node_id)
            .expect("toml node id not found; this is a bug")
    }

    fn wrap(&self, n: Node<'_>) -> TomlNode {
        TomlNode {
            inner: self.inner.clone(),
            node_id: n.id(),
            is_document_root: false,
        }
    }
}

impl DataNode for TomlNode {
    fn kind(&self) -> DataKind {
        if self.is_document_root {
            return DataKind::Object;
        }
        toml_kind(self.node())
    }

    fn byte_range(&self) -> (u32, u32) {
        let n = self.node();
        (n.start_byte() as u32, n.end_byte() as u32)
    }

    fn as_scalar_text(&self) -> Option<Cow<'_, str>> {
        if self.is_document_root {
            return None;
        }
        toml_scalar_text(self.node(), &self.inner.src)
    }

    fn entries(&self) -> Box<dyn Iterator<Item = (Self, Self)> + '_> {
        let n = self.node();
        let pairs: Vec<_> = match n.kind() {
            "document" => collect_document_entries(n, self),
            "table" | "table_array_element" => collect_table_entries(n, self),
            "inline_table" => collect_inline_table_entries(n, self),
            _ => vec![],
        };
        Box::new(pairs.into_iter())
    }

    fn items(&self) -> Box<dyn Iterator<Item = Self> + '_> {
        let n = self.node();
        let items: Vec<_> = if n.kind() == "array" {
            (0..n.named_child_count())
                .filter_map(|i| Some(self.wrap(n.named_child(i)?)))
                .collect()
        } else {
            vec![]
        };
        Box::new(items.into_iter())
    }

    fn source(&self) -> &[u8] {
        &self.inner.src
    }
}

fn toml_kind(n: Node<'_>) -> DataKind {
    match n.kind() {
        "document" | "table" | "table_array_element" | "inline_table" => DataKind::Object,
        "array" => DataKind::Array,
        _ => DataKind::Scalar,
    }
}

fn toml_scalar_text<'a>(n: Node<'_>, src: &'a [u8]) -> Option<Cow<'a, str>> {
    match n.kind() {
        "string" | "quoted_key" => {
            let raw = &src[n.start_byte()..n.end_byte()];
            let s = std::str::from_utf8(raw).ok()?;
            Some(Cow::Owned(unescape_toml_string(s)))
        }
        _ => {
            let raw = &src[n.start_byte()..n.end_byte()];
            Some(Cow::Owned(String::from_utf8_lossy(raw).into_owned()))
        }
    }
}

fn unescape_toml_string(s: &str) -> String {
    if s.starts_with("\"\"\"") {
        unescape_toml_basic(&s[3..s.len() - 3])
    } else if s.starts_with('"') {
        unescape_toml_basic(&s[1..s.len() - 1])
    } else if s.starts_with("'''") {
        s[3..s.len() - 3].to_owned()
    } else if s.starts_with('\'') {
        s[1..s.len() - 1].to_owned()
    } else {
        s.to_owned()
    }
}

fn unescape_toml_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('b') => out.push('\x08'),
                Some('t') => out.push('\t'),
                Some('n') => out.push('\n'),
                Some('f') => out.push('\x0C'),
                Some('r') => out.push('\r'),
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('u') => {
                    let hex: String = chars.by_ref().take(4).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                }
                Some('U') => {
                    let hex: String = chars.by_ref().take(8).collect();
                    if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                        if let Some(ch) = char::from_u32(cp) {
                            out.push(ch);
                        }
                    }
                }
                Some(c) => {
                    out.push('\\');
                    out.push(c);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn collect_document_entries(n: Node<'_>, owner: &TomlNode) -> Vec<(TomlNode, TomlNode)> {
    let mut pairs = Vec::new();
    for i in 0..n.named_child_count() {
        let child = match n.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        match child.kind() {
            "pair" => {
                if let Some(p) = extract_pair(child, owner) {
                    pairs.push(p);
                }
            }
            "table" | "table_array_element" => {
                if let Some(key_node) = table_header_key(child, owner) {
                    pairs.push((key_node, owner.wrap(child)));
                }
            }
            _ => {}
        }
    }
    pairs
}

fn collect_table_entries(n: Node<'_>, owner: &TomlNode) -> Vec<(TomlNode, TomlNode)> {
    let mut pairs = Vec::new();
    for i in 0..n.named_child_count() {
        let child = match n.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "pair" {
            if let Some(p) = extract_pair(child, owner) {
                pairs.push(p);
            }
        }
    }
    pairs
}

fn collect_inline_table_entries(n: Node<'_>, owner: &TomlNode) -> Vec<(TomlNode, TomlNode)> {
    let mut pairs = Vec::new();
    for i in 0..n.named_child_count() {
        let child = match n.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "pair" {
            if let Some(p) = extract_pair(child, owner) {
                pairs.push(p);
            }
        }
    }
    pairs
}

fn extract_pair(pair: Node<'_>, owner: &TomlNode) -> Option<(TomlNode, TomlNode)> {
    let mut key: Option<Node<'_>> = None;
    let mut val: Option<Node<'_>> = None;
    for i in 0..pair.named_child_count() {
        let c = match pair.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        match c.kind() {
            "bare_key" | "dotted_key" | "quoted_key" => {
                if key.is_none() {
                    key = Some(c);
                }
            }
            _ => {
                if val.is_none() {
                    val = Some(c);
                }
            }
        }
    }
    Some((owner.wrap(key?), owner.wrap(val?)))
}

fn table_header_key(table: Node<'_>, owner: &TomlNode) -> Option<TomlNode> {
    for i in 0..table.named_child_count() {
        let c = match table.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        match c.kind() {
            "bare_key" | "dotted_key" | "quoted_key" => return Some(owner.wrap(c)),
            _ => {}
        }
    }
    None
}

fn find_node_by_id<'a>(root: Node<'a>, id: usize) -> Option<Node<'a>> {
    if root.id() == id {
        return Some(root);
    }
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if let Some(found) = find_node_by_id(child, id) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> TomlNode {
        TomlNode::parse(Arc::from(s.as_bytes())).unwrap()
    }

    #[test]
    fn root_pairs() {
        let src = "name = \"sprefa\"\nversion = \"0.1.0\"\n";
        let root = parse(src);
        assert_eq!(root.kind(), DataKind::Object);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].0.as_scalar_text().unwrap().as_ref(), "name");
        assert_eq!(entries[0].1.as_scalar_text().unwrap().as_ref(), "sprefa");
        assert_eq!(entries[1].1.as_scalar_text().unwrap().as_ref(), "0.1.0");
    }

    #[test]
    fn table_section() {
        let src = "[package]\nname = \"foo\"\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        assert_eq!(entries.len(), 1);
        let (k, v) = &entries[0];
        assert_eq!(k.as_scalar_text().unwrap().as_ref(), "package");
        assert_eq!(v.kind(), DataKind::Object);
        let inner: Vec<_> = v.entries().collect();
        assert_eq!(inner[0].0.as_scalar_text().unwrap().as_ref(), "name");
        assert_eq!(inner[0].1.as_scalar_text().unwrap().as_ref(), "foo");
    }

    #[test]
    fn array_value() {
        let src = "nums = [1, 2, 3]\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, arr) = &entries[0];
        assert_eq!(arr.kind(), DataKind::Array);
        let items: Vec<_> = arr.items().collect();
        assert_eq!(items.len(), 3);
    }

    #[test]
    fn inline_table() {
        let src = "point = {x = 1, y = 2}\n";
        let root = parse(src);
        let entries: Vec<_> = root.entries().collect();
        let (_, v) = &entries[0];
        assert_eq!(v.kind(), DataKind::Object);
        let inner: Vec<_> = v.entries().collect();
        assert_eq!(inner.len(), 2);
    }
}
