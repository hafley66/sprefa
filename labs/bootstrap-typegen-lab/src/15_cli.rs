use std::path::{Path, PathBuf};

use crate::check::check;
use crate::codegen_js::{emit_client, emit_models as emit_js_models, emit_smoke};
use crate::codegen_rust::emit_models;
use crate::facts::FactStore;
use crate::parser::parse;
use crate::rules::saturate;
use crate::source::Source;
use crate::store::Store;
use crate::types::{Primitive, Type, Value};
use crate::SourceId;

pub fn compile(path: &Path) -> Result<Store, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let source = Source {
        id: SourceId(0),
        text,
    };
    let parsed = parse(&source);
    Store::new(source)
        .lower(parsed)
        .map_err(|errors| errors.join("\n"))
}

pub fn generate(store: &Store, output: &Path) -> Result<(), String> {
    check(store).map_err(|errors| errors.join("\n"))?;
    std::fs::create_dir_all(output).map_err(|error| error.to_string())?;
    std::fs::write(output.join("models.rs"), emit_models(store))
        .map_err(|error| error.to_string())?;
    std::fs::write(output.join("server.rs"), server_source(store))
        .map_err(|error| error.to_string())?;
    std::fs::write(output.join("models.mjs"), emit_js_models(store))
        .map_err(|error| error.to_string())?;
    std::fs::write(output.join("client.mjs"), emit_client(store))
        .map_err(|error| error.to_string())?;
    std::fs::write(output.join("client-smoke.mjs"), emit_smoke(store))
        .map_err(|error| error.to_string())?;
    let mut facts = FactStore::default();
    let count = saturate(store, &mut facts);
    std::fs::write(
        output.join("facts.txt"),
        format!("inserted={count}\n{:#?}\n", facts.facts),
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn server_source(store: &Store) -> String {
    let mut out = emit_models(store);
    out.push_str(
        r#"use std::io::{Read, Write};
use std::net::TcpListener;

#[derive(Clone, Copy)]
enum SlotKind {
    String,
    Int,
    Bool,
    Literal(&'static str),
    OneOf(&'static [&'static str]),
}

fn slot_matches(kind: SlotKind, value: &str) -> bool {
    match kind {
        SlotKind::String => !value.is_empty(),
        SlotKind::Int => value.parse::<i64>().is_ok(),
        SlotKind::Bool => value.parse::<bool>().is_ok(),
        SlotKind::Literal(expected) => value == expected,
        SlotKind::OneOf(expected) => expected.iter().any(|item| *item == value),
    }
}

fn template_matches(template: &str, kinds: &[SlotKind], input: &str) -> bool {
    let mut rest = input;
    let mut cursor = 0;
    let mut slot_index = 0;
    while cursor < template.len() {
        let bytes = template.as_bytes();
        if bytes[cursor] == b'{' {
            let Some(end) = template[cursor..].find('}') else { return false; };
            cursor += end + 1;
            let next = template[cursor..].find('{').map(|offset| cursor + offset).unwrap_or(template.len());
            let literal = &template[cursor..next];
            let (capture, remainder) = if literal.is_empty() {
                (rest, "")
            } else {
                let Some(value_end) = rest.find(literal) else { return false; };
                (&rest[..value_end], &rest[value_end + literal.len()..])
            };
            let Some(kind) = kinds.get(slot_index).copied() else { return false; };
            if !slot_matches(kind, capture) { return false; }
            slot_index += 1;
            rest = remainder;
            cursor = next;
        } else {
            let next = template[cursor..].find('{').map(|offset| cursor + offset).unwrap_or(template.len());
            if !rest.starts_with(&template[cursor..next]) { return false; }
            rest = &rest[next - cursor..];
            cursor = next;
        }
    }
    rest.is_empty() && slot_index == kinds.len()
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4000").unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap(); let mut request = [0; 4096]; let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]); let mut words = request.split_whitespace(); let method = words.next().unwrap_or(""); let path = words.next().unwrap_or("/");
        let (status, body) = route(method, path); let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()); stream.write_all(response.as_bytes()).unwrap();
    }
}

"#,
    );
    out.push_str("fn route(method: &str, path: &str) -> (&'static str, String) {\n");
    for (domain, operation, pattern, output) in &store.consumers {
        if domain == "http" {
            let template = normalized_template(store, *pattern);
            let body = json_value(store, *output);
            let kinds = slot_kinds(store, *pattern).join(", ");
            out.push_str(&format!("    if method == \"{}\" && template_matches(\"{}\", &[{}], path) {{ return (\"200 OK\", r#\"{}\"#.to_owned()); }}\n", operation.to_ascii_uppercase(), template, kinds, body));
        }
    }
    out.push_str("    (\"404 Not Found\", r#\"{\"error\":\"not found\"}\"#.to_owned())\n}\n");
    out.push_str(
        "\n#[cfg(test)]\nmod generated_matcher_tests {\n    use super::*;\n\n    #[test]\n    fn typed_routes_are_deterministic() {\n",
    );
    for (domain, _, pattern, _) in &store.consumers {
        if domain == "http" {
            let template = normalized_template(store, *pattern);
            let kinds = slot_kinds(store, *pattern);
            out.push_str(&format!(
                "        assert!(template_matches(\"{}\", &[{}], \"{}\"));\n",
                template,
                kinds.join(", "),
                valid_template_input(store, *pattern)
            ));
            if kinds.iter().any(|kind| kind != "SlotKind::String") {
                out.push_str(&format!(
                    "        assert!(!template_matches(\"{}\", &[{}], \"{}\"));\n",
                    template,
                    kinds.join(", "),
                    invalid_template_input(store, *pattern)
                ));
            }
        }
    }
    out.push_str("    }\n}\n");
    out
}

fn normalized_template(store: &Store, id: crate::PatternId) -> String {
    store.patterns[id.0 as usize]
        .parts
        .iter()
        .map(|part| match part {
            crate::PatternPart::Literal { text, .. } => text.clone(),
            crate::PatternPart::Slot(slot) => {
                format!("{{{}}}", store.symbols.resolve(slot.name.unwrap()))
            }
        })
        .collect()
}

fn slot_kinds(store: &Store, id: crate::PatternId) -> Vec<String> {
    store.patterns[id.0 as usize]
        .parts
        .iter()
        .filter_map(|part| match part {
            crate::PatternPart::Slot(slot) => Some(slot_kind(store, slot.ty)),
            _ => None,
        })
        .collect()
}

fn slot_kind(store: &Store, id: crate::TypeId) -> String {
    match &store.types[id.0 as usize] {
        Type::Alias { target, .. } => slot_kind(store, *target),
        Type::Primitive(Primitive::Int) => "SlotKind::Int".to_owned(),
        Type::Primitive(Primitive::Bool) => "SlotKind::Bool".to_owned(),
        Type::Literal(Value::String(value)) => format!("SlotKind::Literal(\"{value}\")"),
        Type::Union(items) => {
            let values = items
                .iter()
                .filter_map(|item| match &store.types[item.0 as usize] {
                    Type::Literal(Value::String(value)) => Some(format!("\"{value}\"")),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if values.len() == items.len() {
                format!("SlotKind::OneOf(&[{}])", values.join(", "))
            } else {
                "SlotKind::String".to_owned()
            }
        }
        _ => "SlotKind::String".to_owned(),
    }
}

fn valid_template_input(store: &Store, id: crate::PatternId) -> String {
    render_template_input(store, id, false)
}

fn invalid_template_input(store: &Store, id: crate::PatternId) -> String {
    render_template_input(store, id, true)
}

fn render_template_input(store: &Store, id: crate::PatternId, invalid: bool) -> String {
    store.patterns[id.0 as usize]
        .parts
        .iter()
        .map(|part| match part {
            crate::PatternPart::Literal { text, .. } => text.clone(),
            crate::PatternPart::Slot(slot) => render_type_sample(store, slot.ty, invalid),
        })
        .collect()
}

fn render_type_sample(store: &Store, id: crate::TypeId, invalid: bool) -> String {
    match &store.types[id.0 as usize] {
        Type::Alias { target, .. } => render_type_sample(store, *target, invalid),
        Type::Primitive(Primitive::Int) => if invalid { "invalid" } else { "42" }.to_owned(),
        Type::Primitive(Primitive::Bool) => if invalid { "maybe" } else { "true" }.to_owned(),
        Type::Literal(Value::String(value)) => {
            if invalid {
                "invalid".to_owned()
            } else {
                value.clone()
            }
        }
        Type::Union(items) => match (items.first(), invalid) {
            (Some(item), false) => render_type_sample(store, *item, false),
            (Some(_), true) => "invalid".to_owned(),
            (None, _) => "invalid".to_owned(),
        },
        _ => if invalid { "invalid" } else { "sample" }.to_owned(),
    }
}
fn json_value(store: &Store, id: crate::TypeId) -> String {
    match &store.types[id.0 as usize] {
        crate::Type::Alias { target, .. } => json_value(store, *target),
        crate::Type::Primitive(crate::Primitive::String)
        | crate::Type::Literal(crate::Value::String(_)) => "\"generated\"".to_owned(),
        crate::Type::Primitive(crate::Primitive::Int)
        | crate::Type::Literal(crate::Value::Int(_)) => "0".to_owned(),
        crate::Type::Primitive(crate::Primitive::Bool)
        | crate::Type::Literal(crate::Value::Bool(_)) => "true".to_owned(),
        crate::Type::Array(_) => "[]".to_owned(),
        crate::Type::Map { .. } => "{}".to_owned(),
        crate::Type::Optional(_) => "null".to_owned(),
        crate::Type::Record(record) => {
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    format!(
                        "\"{}\":{}",
                        store.symbols.resolve(field.name),
                        json_value(store, field.ty)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
        crate::Type::Union(items) => items
            .first()
            .map(|item| json_value(store, *item))
            .unwrap_or_else(|| "null".to_owned()),
        crate::Type::Error => "null".to_owned(),
    }
}

pub fn default_output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/bootstrap-generated")
}
