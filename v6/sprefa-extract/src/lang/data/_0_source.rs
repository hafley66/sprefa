//! json / jsonl / yaml / toml as ONE plane, v5 `src/datapath.rs` ported.
//! One parse feeds both records: a `data_doc` per document carrying it as a json
//! VALUE (what `decode/2` reads), and a `data_value` per value inside it.
//! No second reader and no yaml-to-json pass exists (user 2026-08-17).
// @comment-ok: the header names the ported source and the one-parse constraint

use serde_json::{Map, Value};

use crate::lang::astgrep::AstgrepSource;
use crate::rows::FamilyBundle;
use crate::shape::{Span, Strings};
use crate::source::{ExtractOutput, FamilyMask, Source};
use crate::trace;
use crate::types::{DataDoc, DataF, DataFAux, DataFormat, DataValueKind, DataValueRow};

#[derive(Default)]
pub struct DataSource;

impl Source for DataSource {
    fn name(&self) -> &'static str {
        "data"
    }

    fn matches(&self, path: &str) -> bool {
        matches!(
            path.rsplit('.').next().unwrap_or(""),
            "json" | "jsonl" | "ndjson" | "yaml" | "yml" | "toml"
        )
    }

    fn extract(&self, path: &str, content: &[u8], mask: FamilyMask) -> ExtractOutput {
        let mut out = if mask.cst {
            AstgrepSource.extract(
                path,
                content,
                FamilyMask {
                    cst: true,
                    ..FamilyMask::NONE
                },
            )
        } else {
            ExtractOutput::default()
        };
        if mask.data {
            let span = trace::family_span("data", "data");
            let _entered = span.enter();
            out.data = data_bundle(path, content, &mut out.strings);
            if let Some(bundle) = &out.data {
                trace::record_bundle(&span, bundle, bundle.aux.values.len());
            }
        }
        out
    }
}

/// One file's data plane. None when the bytes are not utf-8 or the grammar
/// refuses to load; an unparseable document is an ERROR tree, which still walks.
fn data_bundle(path: &str, content: &[u8], strings: &mut Strings) -> Option<FamilyBundle<DataF>> {
    let text = std::str::from_utf8(content).ok()?;
    let format = DataFormat::of_path(path);
    let mut aux = DataFAux {
        format,
        docs: Vec::new(),
        values: Vec::new(),
    };
    if format == DataFormat::Jsonl {
        collect_jsonl(text, strings, &mut aux);
    } else {
        collect_single(format, text, strings, &mut aux)?;
    }
    Some(FamilyBundle {
        nodes: Vec::new(),
        edges: Vec::new(),
        aux,
    })
}

fn parse(language: tree_sitter::Language, text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&language).ok()?;
    parser.parse(text, None)
}

fn language_of(format: DataFormat) -> tree_sitter::Language {
    match format {
        DataFormat::Json | DataFormat::Jsonl => tree_sitter_json::LANGUAGE.into(),
        DataFormat::Yaml => tree_sitter_yaml::LANGUAGE.into(),
        DataFormat::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
    }
}

fn collect_single(
    format: DataFormat,
    text: &str,
    strings: &mut Strings,
    aux: &mut DataFAux,
) -> Option<()> {
    let tree = parse(language_of(format), text)?;
    let src = text.as_bytes();
    for (ordinal, root) in root_values(format, tree.root_node())
        .into_iter()
        .enumerate()
    {
        let ordinal = ordinal as u32;
        aux.docs.push(DataDoc {
            ordinal,
            span: span_of(root, 0),
            value: to_json(format, root, src),
        });
        walk(format, root, 0, ordinal, &mut Vec::new(), src, strings, aux);
    }
    Some(())
}

/// JSONL: each non-empty line is an independent json document. Spans are
/// offset back to file coordinates (v5 `run_data_jsonl`, datapath.rs:57-88).
fn collect_jsonl(text: &str, strings: &mut Strings, aux: &mut DataFAux) {
    let Some(tree_language) = Some(language_of(DataFormat::Jsonl)) else {
        return;
    };
    let mut line_start = 0usize;
    let mut ordinal = 0u32;
    for line in text.lines() {
        let start_of_line = line_start;
        line_start += line.len() + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some(tree) = parse(tree_language.clone(), trimmed) else {
            continue;
        };
        let leading = line.len() - line.trim_start().len();
        let offset = (start_of_line + leading) as u32;
        let src = trimmed.as_bytes();
        for root in root_values(DataFormat::Jsonl, tree.root_node()) {
            aux.docs.push(DataDoc {
                ordinal,
                span: span_of(root, offset),
                value: to_json(DataFormat::Jsonl, root, src),
            });
            walk(
                DataFormat::Jsonl,
                root,
                offset,
                ordinal,
                &mut Vec::new(),
                src,
                strings,
                aux,
            );
            ordinal += 1;
        }
    }
}

fn span_of(node: tree_sitter::Node, offset: u32) -> Span {
    Span {
        start: offset + node.start_byte() as u32,
        len: (node.end_byte() - node.start_byte()) as u32,
    }
}

/// Objects and arrays carry no text: their span already delimits the subtree,
/// and repeating every ancestor's slice makes the output quadratic in depth.
#[allow(clippy::too_many_arguments)]
fn walk(
    format: DataFormat,
    node: tree_sitter::Node,
    offset: u32,
    doc: u32,
    segments: &mut Vec<String>,
    src: &[u8],
    strings: &mut Strings,
    aux: &mut DataFAux,
) {
    let kind = kind_of(format, node, src);
    let path = strings.intern(&segments.join("."));
    let (text, span) = match kind {
        DataValueKind::Object | DataValueKind::Array => (None, span_of(node, offset)),
        _ => {
            let (text, start, end) = scalar_text_span(format, node, src);
            (
                Some(strings.intern(&text)),
                Span {
                    start: offset + start as u32,
                    len: (end - start) as u32,
                },
            )
        }
    };
    aux.values.push(DataValueRow {
        doc,
        path,
        kind,
        text,
        span,
    });
    for (key_segments, value) in entries(format, node, src) {
        let depth = key_segments.len();
        segments.extend(key_segments);
        walk(format, value, offset, doc, segments, src, strings, aux);
        segments.truncate(segments.len() - depth);
    }
    for (index, item) in items(format, node).into_iter().enumerate() {
        segments.push(index.to_string());
        walk(format, item, offset, doc, segments, src, strings, aux);
        segments.pop();
    }
}

// ── the shared descent (v5 datapath.rs:90-250, ported) ──────────────────────

/// The top-level value node(s) under the grammar's document wrapper. YAML yields
/// one per document in the stream; TOML's root is itself the object.
fn root_values(format: DataFormat, root: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    match format {
        DataFormat::Json | DataFormat::Jsonl => {
            let mut cursor = root.walk();
            root.named_children(&mut cursor).take(1).collect()
        }
        DataFormat::Toml => vec![root],
        DataFormat::Yaml => {
            let mut out = Vec::new();
            let mut cursor = root.walk();
            for document in root.named_children(&mut cursor) {
                if document.kind() != "document" {
                    continue;
                }
                let mut inner = document.walk();
                for node in document.named_children(&mut inner) {
                    if matches!(node.kind(), "block_node" | "flow_node") {
                        out.push(yaml_unwrap(node));
                    }
                }
            }
            out
        }
    }
}

/// Key/value pairs of an object node. Keys come pre-split into segments: one for
/// json/yaml, one per part for a TOML dotted key.
fn entries<'a>(
    format: DataFormat,
    node: tree_sitter::Node<'a>,
    src: &[u8],
) -> Vec<(Vec<String>, tree_sitter::Node<'a>)> {
    let mut out = Vec::new();
    match format {
        DataFormat::Json | DataFormat::Jsonl => {
            if node.kind() != "object" {
                return out;
            }
            let mut cursor = node.walk();
            for pair in node.named_children(&mut cursor) {
                if pair.kind() != "pair" {
                    continue;
                }
                if let (Some(key), Some(value)) = (
                    pair.child_by_field_name("key"),
                    pair.child_by_field_name("value"),
                ) {
                    out.push((vec![json_str_value(key, src)], value));
                }
            }
        }
        DataFormat::Yaml => {
            if !matches!(node.kind(), "block_mapping" | "flow_mapping") {
                return out;
            }
            let mut cursor = node.walk();
            for pair in node.named_children(&mut cursor) {
                if !matches!(pair.kind(), "block_mapping_pair" | "flow_pair") {
                    continue;
                }
                if let (Some(key), Some(value)) = (
                    pair.child_by_field_name("key"),
                    pair.child_by_field_name("value"),
                ) {
                    out.push((
                        vec![yaml_scalar_text(yaml_unwrap(key), src)],
                        yaml_unwrap(value),
                    ));
                }
            }
        }
        DataFormat::Toml => match node.kind() {
            // the document's entries are its top-level pairs plus each [table] /
            // [[table]] header as a (possibly dotted) key for the table node
            "document" => {
                // A `[[table]]` header repeats, so its element index joins the
                // address; without it two elements share one path.
                let mut seen: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    match child.kind() {
                        "pair" => out.extend(toml_pair(child, src)),
                        "table" => {
                            if let Some(key) = toml_key_node(child) {
                                out.push((toml_key_segs(key, src), child));
                            }
                        }
                        "table_array_element" => {
                            if let Some(key) = toml_key_node(child) {
                                let mut segments = toml_key_segs(key, src);
                                let index = seen.entry(segments.join(".")).or_insert(0);
                                segments.push(index.to_string());
                                *index += 1;
                                out.push((segments, child));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "table" | "table_array_element" | "inline_table" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.kind() == "pair" {
                        out.extend(toml_pair(child, src));
                    }
                }
            }
            _ => {}
        },
    }
    out
}

/// Item nodes of an array/sequence node; empty for anything else.
fn items(format: DataFormat, node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut out = Vec::new();
    match format {
        DataFormat::Json | DataFormat::Jsonl | DataFormat::Toml => {
            if node.kind() != "array" {
                return out;
            }
            let mut cursor = node.walk();
            out.extend(node.named_children(&mut cursor));
        }
        DataFormat::Yaml => {
            if !matches!(node.kind(), "block_sequence" | "flow_sequence") {
                return out;
            }
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                match child.kind() {
                    "block_sequence_item" => {
                        if let Some(inner) = child.named_child(0) {
                            out.push(yaml_unwrap(inner));
                        }
                    }
                    "flow_node" => out.push(yaml_unwrap(child)),
                    _ => {}
                }
            }
        }
    }
    out
}

/// A scalar as (text, byte span). String-ish nodes strip their quotes so the span
/// points at the value, mirroring v5's `value_text_span` (datapath.rs:250-280).
fn scalar_text_span(
    format: DataFormat,
    node: tree_sitter::Node,
    src: &[u8],
) -> (String, usize, usize) {
    let (low, high) = (node.start_byte(), node.end_byte());
    let raw = String::from_utf8_lossy(&src[low..high]).into_owned();
    match format {
        DataFormat::Json | DataFormat::Jsonl if node.kind() == "string" && high - low >= 2 => {
            (json_unescape(&raw[1..raw.len() - 1]), low + 1, high - 1)
        }
        DataFormat::Yaml
            if matches!(node.kind(), "double_quote_scalar" | "single_quote_scalar")
                && high - low >= 2 =>
        {
            (yaml_scalar_text(node, src), low + 1, high - 1)
        }
        DataFormat::Toml if node.kind() == "string" => {
            let quote = if raw.starts_with("\"\"\"") || raw.starts_with("'''") {
                3
            } else {
                1
            };
            if raw.len() >= 2 * quote {
                (toml_unquote(&raw), low + quote, high - quote)
            } else {
                (raw, low, high)
            }
        }
        _ => (raw, low, high),
    }
}

// ── value classes ───────────────────────────────────────────────────────────

fn kind_of(format: DataFormat, node: tree_sitter::Node, src: &[u8]) -> DataValueKind {
    match format {
        DataFormat::Json | DataFormat::Jsonl => match node.kind() {
            "object" => DataValueKind::Object,
            "array" => DataValueKind::Array,
            "string" => DataValueKind::String,
            "number" => DataValueKind::Number,
            "true" | "false" => DataValueKind::Boolean,
            _ => DataValueKind::Null,
        },
        DataFormat::Yaml => match node.kind() {
            "block_mapping" | "flow_mapping" => DataValueKind::Object,
            "block_sequence" | "flow_sequence" => DataValueKind::Array,
            "double_quote_scalar" | "single_quote_scalar" | "block_scalar" => DataValueKind::String,
            _ => plain_scalar_kind(&yaml_scalar_text(node, src)),
        },
        DataFormat::Toml => match node.kind() {
            "document" | "table" | "table_array_element" | "inline_table" => DataValueKind::Object,
            "array" => DataValueKind::Array,
            "integer" | "float" => DataValueKind::Number,
            "boolean" => DataValueKind::Boolean,
            _ => DataValueKind::String,
        },
    }
}

/// YAML 1.2 core-schema resolution of an unquoted scalar.
fn plain_scalar_kind(text: &str) -> DataValueKind {
    match text {
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => DataValueKind::Boolean,
        "null" | "Null" | "NULL" | "~" | "" => DataValueKind::Null,
        _ if text.parse::<i64>().is_ok() || text.parse::<f64>().is_ok() => DataValueKind::Number,
        _ => DataValueKind::String,
    }
}

// ── the json VALUE, off the same parse ──────────────────────────────────────

fn to_json(format: DataFormat, node: tree_sitter::Node, src: &[u8]) -> Value {
    if format == DataFormat::Toml && node.kind() == "document" {
        return toml_document_value(node, src);
    }
    match kind_of(format, node, src) {
        DataValueKind::Object => {
            let mut map = Map::new();
            for (segments, value) in entries(format, node, src) {
                insert_path(&mut map, &segments, to_json(format, value, src));
            }
            Value::Object(map)
        }
        DataValueKind::Array => Value::Array(
            items(format, node)
                .into_iter()
                .map(|item| to_json(format, item, src))
                .collect(),
        ),
        DataValueKind::Null => Value::Null,
        DataValueKind::Boolean => {
            let (text, _, _) = scalar_text_span(format, node, src);
            Value::Bool(matches!(text.as_str(), "true" | "True" | "TRUE"))
        }
        DataValueKind::Number => {
            let (text, _, _) = scalar_text_span(format, node, src);
            serde_json::from_str::<Value>(&text).unwrap_or(Value::String(text))
        }
        DataValueKind::String => {
            let (text, _, _) = scalar_text_span(format, node, src);
            Value::String(text)
        }
    }
}

/// A TOML document folds its top-level pairs, its `[table]`s and its
/// `[[table]]`s into ONE nested object; every other format's root is one node.
fn toml_document_value(node: tree_sitter::Node, src: &[u8]) -> Value {
    let mut root = Map::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "pair" => {
                if let Some((segments, value)) = toml_pair(child, src) {
                    insert_path(&mut root, &segments, to_json(DataFormat::Toml, value, src));
                }
            }
            "table" => {
                if let Some(key) = toml_key_node(child) {
                    let segments = toml_key_segs(key, src);
                    insert_path(&mut root, &segments, to_json(DataFormat::Toml, child, src));
                }
            }
            "table_array_element" => {
                if let Some(key) = toml_key_node(child) {
                    let segments = toml_key_segs(key, src);
                    let element = to_json(DataFormat::Toml, child, src);
                    push_path(&mut root, &segments, element);
                }
            }
            _ => {}
        }
    }
    Value::Object(root)
}

/// Set `segments` (a possibly dotted key) to `value`, creating the objects in
/// between. A segment already holding a non-object is overwritten.
fn insert_path(map: &mut Map<String, Value>, segments: &[String], value: Value) {
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut here = map;
    for segment in parents {
        let slot = here
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            *slot = Value::Object(Map::new());
        }
        here = slot.as_object_mut().expect("just made it an object");
    }
    here.insert(last.clone(), value);
}

/// Append `value` to the array at `segments`, creating it on first sight. The
/// `[[table]]` fold.
fn push_path(map: &mut Map<String, Value>, segments: &[String], value: Value) {
    let Some((last, parents)) = segments.split_last() else {
        return;
    };
    let mut here = map;
    for segment in parents {
        let slot = here
            .entry(segment.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        if !slot.is_object() {
            *slot = Value::Object(Map::new());
        }
        here = slot.as_object_mut().expect("just made it an object");
    }
    let slot = here
        .entry(last.clone())
        .or_insert_with(|| Value::Array(Vec::new()));
    if !slot.is_array() {
        *slot = Value::Array(Vec::new());
    }
    slot.as_array_mut()
        .expect("just made it an array")
        .push(value);
}

// ── json ────────────────────────────────────────────────────────────────────

fn json_str_value(node: tree_sitter::Node, src: &[u8]) -> String {
    let (low, high) = (node.start_byte(), node.end_byte());
    if node.kind() == "string" && high - low >= 2 {
        json_unescape(std::str::from_utf8(&src[low + 1..high - 1]).unwrap_or(""))
    } else {
        String::from_utf8_lossy(&src[low..high]).into_owned()
    }
}

fn json_unescape(text: &str) -> String {
    if !text.contains('\\') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// ── yaml ────────────────────────────────────────────────────────────────────

/// Strip the grammar's block_node/flow_node wrapper (skipping anchor/tag) to the
/// payload mapping/sequence/scalar.
fn yaml_unwrap(node: tree_sitter::Node) -> tree_sitter::Node {
    if !matches!(node.kind(), "block_node" | "flow_node") {
        return node;
    }
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if !matches!(child.kind(), "anchor" | "tag") {
            return child;
        }
    }
    node
}

fn yaml_scalar_text(node: tree_sitter::Node, src: &[u8]) -> String {
    let raw = || String::from_utf8_lossy(&src[node.start_byte()..node.end_byte()]).into_owned();
    match node.kind() {
        "double_quote_scalar" => {
            let text = raw();
            json_unescape(&text[1..text.len().saturating_sub(1)])
        }
        "single_quote_scalar" => {
            let text = raw();
            text[1..text.len().saturating_sub(1)].replace("''", "'")
        }
        "plain_scalar" => match node.named_child(0) {
            Some(child) => yaml_scalar_text(child, src),
            None => raw(),
        },
        _ => raw(),
    }
}

// ── toml ────────────────────────────────────────────────────────────────────

/// A `pair` as (key segments, value node). The key may be dotted.
fn toml_pair<'a>(
    pair: tree_sitter::Node<'a>,
    src: &[u8],
) -> Option<(Vec<String>, tree_sitter::Node<'a>)> {
    let key = toml_key_node(pair)?;
    let mut cursor = pair.walk();
    let value = pair
        .named_children(&mut cursor)
        .filter(|node| {
            !matches!(
                node.kind(),
                "bare_key" | "dotted_key" | "quoted_key" | "comment"
            )
        })
        .last()?;
    Some((toml_key_segs(key, src), value))
}

/// The key node of a pair / table header (first key-kind child).
fn toml_key_node<'a>(node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| matches!(child.kind(), "bare_key" | "dotted_key" | "quoted_key"));
    found
}

/// Key text split into path segments: `a.b."c d"` -> ["a", "b", "c d"].
fn toml_key_segs(key: tree_sitter::Node, src: &[u8]) -> Vec<String> {
    let part = |node: tree_sitter::Node| {
        let raw = String::from_utf8_lossy(&src[node.start_byte()..node.end_byte()]).into_owned();
        if node.kind() == "quoted_key" {
            toml_unquote(&raw)
        } else {
            raw
        }
    };
    if key.kind() != "dotted_key" {
        return vec![part(key)];
    }
    // dotted_key nests left-recursively: (dotted_key (dotted_key a b) c)
    let mut segments = Vec::new();
    let mut stack = vec![key];
    while let Some(node) = stack.pop() {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            if child.kind() == "dotted_key" {
                stack.push(child);
            } else {
                segments.push(part(child));
            }
        }
    }
    segments.reverse();
    segments
}

fn toml_unquote(text: &str) -> String {
    for (open, close) in [
        ("\"\"\"", "\"\"\""),
        ("'''", "'''"),
        ("\"", "\""),
        ("'", "'"),
    ] {
        if text.len() >= open.len() + close.len() && text.starts_with(open) && text.ends_with(close)
        {
            let inner = &text[open.len()..text.len() - close.len()];
            return if open.starts_with('"') {
                json_unescape(inner)
            } else {
                inner.to_string()
            };
        }
    }
    text.to_string()
}
