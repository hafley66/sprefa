use std::path::{Path, PathBuf};

use crate::check::check;
use crate::codegen_js::{emit_client, emit_models as emit_js_models, emit_smoke};
use crate::codegen_rust::emit_models;
use crate::facts::FactStore;
use crate::parser::parse;
use crate::rules::saturate;
use crate::source::Source;
use crate::store::Store;
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
    out.push_str(r#"use std::io::{Read, Write};
use std::net::TcpListener;

fn template_matches(template: &str, input: &str) -> bool {
    let mut rest = input;
    let mut cursor = 0;
    while cursor < template.len() {
        let bytes = template.as_bytes();
        if bytes[cursor] == b'{' {
            let Some(end) = template[cursor..].find('}') else { return false; };
            cursor += end + 1;
            let next = template[cursor..].find('{').map(|offset| cursor + offset).unwrap_or(template.len());
            let literal = &template[cursor..next];
            if literal.is_empty() { return !rest.is_empty(); }
            let Some(value_end) = rest.find(literal) else { return false; };
            rest = &rest[value_end + literal.len()..];
            cursor = next;
        } else {
            let next = template[cursor..].find('{').map(|offset| cursor + offset).unwrap_or(template.len());
            if !rest.starts_with(&template[cursor..next]) { return false; }
            rest = &rest[next - cursor..];
            cursor = next;
        }
    }
    rest.is_empty()
}

fn main() {
    let listener = TcpListener::bind("127.0.0.1:4000").unwrap();
    for stream in listener.incoming() {
        let mut stream = stream.unwrap(); let mut request = [0; 4096]; let size = stream.read(&mut request).unwrap();
        let request = String::from_utf8_lossy(&request[..size]); let mut words = request.split_whitespace(); let method = words.next().unwrap_or(""); let path = words.next().unwrap_or("/");
        let (status, body) = route(method, path); let response = format!("HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()); stream.write_all(response.as_bytes()).unwrap();
    }
}

"#);
    out.push_str("fn route(method: &str, path: &str) -> (&'static str, String) {\n");
    for (domain, operation, pattern, output) in &store.consumers {
        if domain == "http" {
            let template = normalized_template(store, *pattern);
            let body = json_value(store, *output);
            out.push_str(&format!("    if method == \"{}\" && template_matches(\"{}\", path) {{ return (\"200 OK\", r#\"{}\"#.to_owned()); }}\n", operation.to_ascii_uppercase(), template, body));
        }
    }
    out.push_str(
        "    (\"404 Not Found\", r#\"{\\\"error\\\":\\\"not found\\\"}\"#.to_owned())\n}\n",
    );
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
