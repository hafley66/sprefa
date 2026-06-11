//! Dotted-path extraction over data files: the `json` op's evaluator. The op
//! keeps its name but dispatches on file extension (json / yaml,yml / toml),
//! the v4 `AnyDataNode` behavior. Each format parses with its tree-sitter
//! grammar so every extracted value carries a byte span for the ref spine.
//!
//! `*` matches any object key or array index. A TOML dotted key (`a.b = 1`,
//! `[a.b]`) consumes that many path segments. Multi-document YAML streams
//! match the path against every document.

#[derive(Clone, Copy, PartialEq)]
enum Fmt {
    Json,
    Yaml,
    Toml,
}

fn fmt_of(path: &str) -> Fmt {
    match path.rsplit('.').next().unwrap_or("") {
        "yaml" | "yml" => Fmt::Yaml,
        "toml" => Fmt::Toml,
        _ => Fmt::Json,
    }
}

/// Extract leaf values along a dotted path from a json/yaml/toml file.
pub fn run_data(path: &str, content: &str, jpath: &str) -> Vec<(String, usize, usize)> {
    let fmt = fmt_of(path);
    let lang: tree_sitter::Language = match fmt {
        Fmt::Json => tree_sitter_json::LANGUAGE.into(),
        Fmt::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        Fmt::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
    };
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&lang).is_err() { return vec![]; }
    let tree = match parser.parse(content, None) { Some(t) => t, None => return vec![] };
    let src = content.as_bytes();
    let segs: Vec<&str> = jpath.split('.').collect();
    let mut hits: Vec<tree_sitter::Node> = Vec::new();
    for root in root_values(fmt, tree.root_node()) {
        descend(fmt, root, &segs, src, &mut hits);
    }
    hits.iter().map(|n| value_text_span(fmt, *n, content)).collect()
}

/// The top-level value node(s) under the grammar's document wrapper. YAML
/// yields one per document in the stream; TOML's root is itself the object.
fn root_values<'a>(fmt: Fmt, root: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    match fmt {
        Fmt::Json => {
            let mut c = root.walk();
            root.named_children(&mut c).take(1).collect()
        }
        Fmt::Toml => vec![root],
        Fmt::Yaml => {
            let mut out = Vec::new();
            let mut c = root.walk();
            for doc in root.named_children(&mut c) {
                if doc.kind() != "document" { continue; }
                let mut dc = doc.walk();
                for n in doc.named_children(&mut dc) {
                    if matches!(n.kind(), "block_node" | "flow_node") {
                        out.push(yaml_unwrap(n));
                    }
                }
            }
            out
        }
    }
}

/// Match the remaining path segments against a value node. Object entries may
/// consume several segments at once (TOML dotted keys); arrays consume one.
fn descend<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, segs: &[&str], src: &[u8],
    out: &mut Vec<tree_sitter::Node<'a>>,
) {
    if segs.is_empty() {
        out.push(node);
        return;
    }
    for (ksegs, v) in entries(fmt, node, src) {
        if ksegs.len() <= segs.len()
            && ksegs.iter().zip(segs).all(|(k, s)| *s == "*" || k == s)
        {
            descend(fmt, v, &segs[ksegs.len()..], src, out);
        }
    }
    let items = items(fmt, node);
    if segs[0] == "*" {
        for it in items { descend(fmt, it, &segs[1..], src, out); }
    } else if let Ok(idx) = segs[0].parse::<usize>() {
        if let Some(it) = items.get(idx) { descend(fmt, *it, &segs[1..], src, out); }
    }
}

/// Key/value pairs of an object node. Keys come pre-split into segments: one
/// for json/yaml, one per part for a TOML dotted key.
fn entries<'a>(
    fmt: Fmt, node: tree_sitter::Node<'a>, src: &[u8],
) -> Vec<(Vec<String>, tree_sitter::Node<'a>)> {
    let mut out = Vec::new();
    match fmt {
        Fmt::Json => {
            if node.kind() != "object" { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if pair.kind() != "pair" { continue; }
                if let (Some(k), Some(v)) = (pair.child_by_field_name("key"), pair.child_by_field_name("value")) {
                    out.push((vec![json_str_value(k, src)], v));
                }
            }
        }
        Fmt::Yaml => {
            if !matches!(node.kind(), "block_mapping" | "flow_mapping") { return out; }
            let mut c = node.walk();
            for pair in node.named_children(&mut c) {
                if !matches!(pair.kind(), "block_mapping_pair" | "flow_pair") { continue; }
                if let (Some(k), Some(v)) = (pair.child_by_field_name("key"), pair.child_by_field_name("value")) {
                    out.push((vec![yaml_scalar_text(yaml_unwrap(k), src)], yaml_unwrap(v)));
                }
            }
        }
        Fmt::Toml => match node.kind() {
            // the document's entries are its top-level pairs plus each [table]
            // / [[table]] header as a (possibly dotted) key for the table node
            "document" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    match child.kind() {
                        "pair" => out.extend(toml_pair(child, src)),
                        "table" | "table_array_element" => {
                            if let Some(k) = toml_key_node(child) {
                                out.push((toml_key_segs(k, src), child));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "table" | "table_array_element" | "inline_table" => {
                let mut c = node.walk();
                for child in node.named_children(&mut c) {
                    if child.kind() == "pair" { out.extend(toml_pair(child, src)); }
                }
            }
            _ => {}
        },
    }
    out
}

/// Item nodes of an array/sequence node; empty for anything else.
fn items<'a>(fmt: Fmt, node: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut out = Vec::new();
    match fmt {
        Fmt::Json | Fmt::Toml => {
            if node.kind() != "array" { return out; }
            let mut c = node.walk();
            out.extend(node.named_children(&mut c));
        }
        Fmt::Yaml => {
            if !matches!(node.kind(), "block_sequence" | "flow_sequence") { return out; }
            let mut c = node.walk();
            for child in node.named_children(&mut c) {
                match child.kind() {
                    "block_sequence_item" => {
                        if let Some(inner) = child.named_child(0) { out.push(yaml_unwrap(inner)); }
                    }
                    "flow_node" => out.push(yaml_unwrap(child)),
                    _ => {}
                }
            }
        }
    }
    out
}

/// A matched leaf as (text, byte span). String-ish nodes strip their quotes so
/// `ref` points at the value, mirroring the json behavior.
fn value_text_span(fmt: Fmt, n: tree_sitter::Node, content: &str) -> (String, usize, usize) {
    let (lo, hi) = (n.start_byte(), n.end_byte());
    let raw = &content[lo..hi];
    match fmt {
        Fmt::Json if n.kind() == "string" && hi - lo >= 2 =>
            (json_unescape(&raw[1..raw.len() - 1]), lo + 1, hi - 1),
        Fmt::Yaml if matches!(n.kind(), "double_quote_scalar" | "single_quote_scalar") && hi - lo >= 2 =>
            (yaml_scalar_text(n, content.as_bytes()), lo + 1, hi - 1),
        Fmt::Toml if n.kind() == "string" => {
            let q = if raw.starts_with("\"\"\"") || raw.starts_with("'''") { 3 } else { 1 };
            if raw.len() >= 2 * q {
                (toml_unquote(raw), lo + q, hi - q)
            } else {
                (raw.to_string(), lo, hi)
            }
        }
        _ => (raw.to_string(), lo, hi),
    }
}

// ── json ─────────────────────────────────────────────────────────────────────

/// Text of a json string node with quotes stripped and escapes resolved.
fn json_str_value(n: tree_sitter::Node, src: &[u8]) -> String {
    let (lo, hi) = (n.start_byte(), n.end_byte());
    if n.kind() == "string" && hi - lo >= 2 {
        json_unescape(std::str::from_utf8(&src[lo + 1..hi - 1]).unwrap_or(""))
    } else {
        String::from_utf8_lossy(&src[lo..hi]).into_owned()
    }
}

fn json_unescape(s: &str) -> String {
    if !s.contains('\\') { return s.to_string(); }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' { out.push(c); continue; }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some(o) => { out.push('\\'); out.push(o); }
            None => out.push('\\'),
        }
    }
    out
}

// ── yaml ─────────────────────────────────────────────────────────────────────

/// Strip the grammar's block_node/flow_node wrapper (skipping anchor/tag) to
/// the payload mapping/sequence/scalar.
fn yaml_unwrap(n: tree_sitter::Node) -> tree_sitter::Node {
    if !matches!(n.kind(), "block_node" | "flow_node") { return n; }
    let mut c = n.walk();
    for child in n.named_children(&mut c) {
        if !matches!(child.kind(), "anchor" | "tag") { return child; }
    }
    n
}

fn yaml_scalar_text(n: tree_sitter::Node, src: &[u8]) -> String {
    let raw = || String::from_utf8_lossy(&src[n.start_byte()..n.end_byte()]).into_owned();
    match n.kind() {
        "double_quote_scalar" => {
            let s = raw();
            json_unescape(&s[1..s.len().saturating_sub(1)])
        }
        "single_quote_scalar" => {
            let s = raw();
            s[1..s.len().saturating_sub(1)].replace("''", "'")
        }
        "plain_scalar" => match n.named_child(0) {
            Some(c) => yaml_scalar_text(c, src),
            None => raw(),
        },
        _ => raw(),
    }
}

// ── toml ─────────────────────────────────────────────────────────────────────

/// A `pair` as (key segments, value node). The key may be dotted.
fn toml_pair<'a>(pair: tree_sitter::Node<'a>, src: &[u8]) -> Option<(Vec<String>, tree_sitter::Node<'a>)> {
    let k = toml_key_node(pair)?;
    let mut c = pair.walk();
    let v = pair.named_children(&mut c)
        .filter(|n| !matches!(n.kind(), "bare_key" | "dotted_key" | "quoted_key" | "comment"))
        .last()?;
    Some((toml_key_segs(k, src), v))
}

/// The key node of a pair / table header (first key-kind child).
fn toml_key_node(n: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut c = n.walk();
    let found = n.named_children(&mut c)
        .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"));
    found
}

/// Key text split into path segments: `a.b."c d"` -> ["a", "b", "c d"].
fn toml_key_segs(k: tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let part = |n: tree_sitter::Node| {
        let raw = String::from_utf8_lossy(&src[n.start_byte()..n.end_byte()]).into_owned();
        if n.kind() == "quoted_key" { toml_unquote(&raw) } else { raw }
    };
    if k.kind() == "dotted_key" {
        let mut segs = Vec::new();
        let mut stack = vec![k];
        // dotted_key nests left-recursively: (dotted_key (dotted_key a b) c)
        while let Some(n) = stack.pop() {
            let mut c = n.walk();
            let kids: Vec<_> = n.named_children(&mut c).collect();
            for child in kids.into_iter().rev() {
                if child.kind() == "dotted_key" { stack.push(child); } else { segs.push(part(child)); }
            }
        }
        segs.reverse();
        segs
    } else {
        vec![part(k)]
    }
}

fn toml_unquote(s: &str) -> String {
    for (open, close) in [("\"\"\"", "\"\"\""), ("'''", "'''"), ("\"", "\""), ("'", "'")] {
        if s.len() >= open.len() + close.len() && s.starts_with(open) && s.ends_with(close) {
            let inner = &s[open.len()..s.len() - close.len()];
            return if open.starts_with('"') { json_unescape(inner) } else { inner.to_string() };
        }
    }
    s.to_string()
}
