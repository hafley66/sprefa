//! `load_source_fact_files/3` and `install_source_fact_graph/6`. Port of
//! `v7/src/2_comptime/0d_source_fact_loader.pl`.
//!
//! The compact envelope sprefa-extract emits expands into shared content-span
//! and located-occurrence identities before the rows enter the fixpoint.

use super::api::{diagnostic, sorted, Installed, Loaded};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::{Map, Value};

/// `:14-27`, in clause order, which is the order the relation rows and the
/// relation edges come out in.
pub const SOURCE_RELATIONS: [(&str, i64); 14] = [
    ("source", 1),
    ("source_directory", 3),
    ("source_git", 4),
    ("source_revision", 1),
    ("source_content", 2),
    ("content", 2),
    ("git_blob", 3),
    ("parse", 6),
    ("source_query", 3),
    ("content_span", 4),
    ("located", 3),
    ("source_match", 4),
    ("source_capture", 4),
    ("source_replacement", 4),
];

/// `:33`. Every file is one JSON array of protocol-1 envelopes; equal logical
/// rows from several query engines collapse in the sort.
pub fn load_source_fact_texts(u: &mut Universe, files: &[(TermId, String)]) -> Loaded {
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (path, text) in files {
        let Ok(value) = serde_json::from_str::<Value>(text) else {
            // :51. PLAN fork 4: v7 puts the raw swipl exception here.
            let illegal = u.atom("illegal_json");
            let json = u.compound("json", vec![illegal]);
            let syntax = u.compound("syntax_error", vec![json]);
            let payload = u.compound("source_fact_read_error", vec![syntax]);
            let subject = u.compound("file", vec![*path]);
            diagnostics.push(diagnostic(u, "source", subject, payload));
            continue;
        };
        let Some(envelopes) = value.as_array() else {
            // :61.
            let payload = u.atom("source_fact_root_not_array");
            let subject = u.compound("file", vec![*path]);
            diagnostics.push(diagnostic(u, "source", subject, payload));
            continue;
        };
        for (position, envelope) in envelopes.iter().enumerate() {
            match decode_envelope(u, envelope) {
                Some(envelope_rows) => rows.extend(envelope_rows),
                None => {
                    // :77.
                    let position = u.int(position as i64);
                    let payload = u.compound("malformed_source_envelope", vec![position]);
                    let subject = u.compound("file", vec![*path]);
                    diagnostics.push(diagnostic(u, "source", subject, payload));
                }
            }
        }
    }
    Loaded {
        rows: sorted(u, rows),
        diagnostics: sorted(u, diagnostics),
    }
}

fn object(value: &Value) -> Option<&Map<String, Value>> {
    value.as_object()
}

fn field_string<'a>(dict: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    match dict.get(key)? {
        Value::String(s) => Some(s),
        _ => None,
    }
}

fn field_integer(dict: &Map<String, Value>, key: &str) -> Option<i64> {
    dict.get(key)?.as_i64()
}

fn row(u: &mut Universe, name: &str, arguments: Vec<TermId>) -> TermId {
    let name = u.atom(name);
    let list = u.list(&arguments);
    u.compound("source_row", vec![name, list])
}

fn reference(u: &mut Universe, term: TermId) -> TermId {
    u.compound("ref", vec![term])
}

fn constant(u: &mut Universe, term: TermId) -> TermId {
    u.compound("const", vec![term])
}

/// `:87-115`.
fn decode_envelope(u: &mut Universe, envelope: &Value) -> Option<Vec<TermId>> {
    let dict = object(envelope)?;
    if field_integer(dict, "protocol")? != 1 {
        return None;
    }
    let (source, mut rows) = source_rows(u, object(dict.get("source")?)?)?;
    let digest = u.atom(field_string(dict, "content")?);
    let content = u.compound("content", vec![digest]);
    let byte_length = u.int(field_integer(dict, "byte_length")?);

    let git_rows = git_blob_rows(u, dict.get("git_blobs")?.as_array()?, content)?;
    let (parse, parse_row) = parse_row(u, object(dict.get("parse")?)?, content)?;
    let (query, query_row) = query_row(u, object(dict.get("query")?)?)?;
    let match_rows = match_rows(
        u,
        dict.get("matches")?.as_array()?,
        source,
        content,
        parse,
        query,
    )?;

    let source_reference = reference(u, source);
    let content_reference = reference(u, content);
    rows.push(row(
        u,
        "source_content",
        vec![source_reference, content_reference],
    ));
    let length = constant(u, byte_length);
    rows.push(row(u, "content", vec![content_reference, length]));
    rows.push(parse_row);
    rows.push(query_row);
    rows.extend(git_rows);
    rows.extend(match_rows);
    Some(rows)
}

/// `:117-137`.
fn source_rows(u: &mut Universe, dict: &Map<String, Value>) -> Option<(TermId, Vec<TermId>)> {
    match field_string(dict, "kind")? {
        "directory" => {
            let directory = u.string(field_string(dict, "directory")?);
            let path = u.string(field_string(dict, "path")?);
            let source = u.compound("directory_source", vec![directory, path]);
            let source_reference = reference(u, source);
            let directory = constant(u, directory);
            let path = constant(u, path);
            let rows = vec![
                row(u, "source", vec![source_reference]),
                row(
                    u,
                    "source_directory",
                    vec![source_reference, directory, path],
                ),
            ];
            Some((source, rows))
        }
        "git" => {
            let repository = u.string(field_string(dict, "repository")?);
            let path = u.string(field_string(dict, "path")?);
            let revision = source_revision(u, object(dict.get("revision")?)?)?;
            let source = u.compound("git_source", vec![repository, revision, path]);
            let source_reference = reference(u, source);
            let revision_reference = reference(u, revision);
            let repository = constant(u, repository);
            let path = constant(u, path);
            let rows = vec![
                row(u, "source", vec![source_reference]),
                row(u, "source_revision", vec![revision_reference]),
                row(
                    u,
                    "source_git",
                    vec![source_reference, repository, revision_reference, path],
                ),
            ];
            Some((source, rows))
        }
        _ => None,
    }
}

/// `:139-147`.
fn source_revision(u: &mut Universe, dict: &Map<String, Value>) -> Option<TermId> {
    match field_string(dict, "kind")? {
        "commit" => {
            let object = u.string(field_string(dict, "object")?);
            Some(u.compound("commit", vec![object]))
        }
        "worktree" => {
            let worktree = u.string(field_string(dict, "worktree")?);
            let head = match dict.get("head")? {
                Value::Null => u.atom("none"),
                Value::String(text) => {
                    let text = u.string(text);
                    u.compound("some", vec![text])
                }
                _ => return None,
            };
            let dirty = match dict.get("dirty")? {
                Value::Bool(true) => u.atom("true"),
                Value::Bool(false) => u.atom("false"),
                _ => return None,
            };
            Some(u.compound("worktree", vec![worktree, head, dirty]))
        }
        _ => None,
    }
}

/// `:149-156`.
fn git_blob_rows(u: &mut Universe, blobs: &[Value], content: TermId) -> Option<Vec<TermId>> {
    let mut rows = Vec::with_capacity(blobs.len());
    for blob in blobs {
        let dict = object(blob)?;
        let repository = u.string(field_string(dict, "repository")?);
        let object_name = u.string(field_string(dict, "object")?);
        let content_reference = reference(u, content);
        let repository = constant(u, repository);
        let object_name = constant(u, object_name);
        rows.push(row(
            u,
            "git_blob",
            vec![content_reference, repository, object_name],
        ));
    }
    Some(rows)
}

/// `:158-168`.
fn parse_row(
    u: &mut Universe,
    dict: &Map<String, Value>,
    content: TermId,
) -> Option<(TermId, TermId)> {
    let grammar = u.atom(field_string(dict, "grammar")?);
    let engine = u.atom(field_string(dict, "engine")?);
    let version = u.string(field_string(dict, "version")?);
    let configuration = json_term(u, dict.get("configuration")?)?;
    let parse = u.compound(
        "parse",
        vec![content, grammar, engine, version, configuration],
    );
    let parse_reference = reference(u, parse);
    let content_reference = reference(u, content);
    let grammar = constant(u, grammar);
    let engine = constant(u, engine);
    let version = constant(u, version);
    let configuration = constant(u, configuration);
    let parse_row = row(
        u,
        "parse",
        vec![
            parse_reference,
            content_reference,
            grammar,
            engine,
            version,
            configuration,
        ],
    );
    Some((parse, parse_row))
}

/// `:170-176`.
fn query_row(u: &mut Universe, dict: &Map<String, Value>) -> Option<(TermId, TermId)> {
    let engine = u.atom(field_string(dict, "engine")?);
    let specification = json_term(u, dict.get("specification")?)?;
    let query = u.compound("source_query", vec![engine, specification]);
    let query_reference = reference(u, query);
    let engine = constant(u, engine);
    let specification = constant(u, specification);
    let query_row = row(
        u,
        "source_query",
        vec![query_reference, engine, specification],
    );
    Some((query, query_row))
}

/// `:178-196`.
fn match_rows(
    u: &mut Universe,
    matches: &[Value],
    source: TermId,
    content: TermId,
    parse: TermId,
    query: TermId,
) -> Option<Vec<TermId>> {
    let mut rows = Vec::new();
    for entry in matches {
        let dict = object(entry)?;
        let position = u.int(field_integer(dict, "position")?);
        let branch = u.atom(field_string(dict, "branch")?);
        let pattern = u.int(field_integer(dict, "pattern")?);
        let (span, span_rows) = range_identity(u, object(dict.get("range")?)?, content)?;
        let matched = u.compound(
            "source_match",
            vec![query, parse, position, branch, pattern, span],
        );
        let matched_reference = reference(u, matched);
        let query_reference = reference(u, query);
        let parse_reference = reference(u, parse);
        let span_reference = reference(u, span);
        let match_row = row(
            u,
            "source_match",
            vec![
                matched_reference,
                query_reference,
                parse_reference,
                span_reference,
            ],
        );
        let capture_rows = capture_rows(
            u,
            dict.get("captures")?.as_array()?,
            matched,
            source,
            content,
        )?;
        let replacement_rows =
            replacement_rows(u, dict.get("replacement")?, matched, source, span)?;
        rows.extend(span_rows);
        rows.push(match_row);
        rows.extend(capture_rows);
        rows.extend(replacement_rows);
    }
    Some(rows)
}

/// `:198-211`.
fn capture_rows(
    u: &mut Universe,
    captures: &[Value],
    matched: TermId,
    source: TermId,
    content: TermId,
) -> Option<Vec<TermId>> {
    let mut rows = Vec::new();
    for capture in captures {
        let dict = object(capture)?;
        let position = u.int(field_integer(dict, "position")?);
        let label = u.atom(field_string(dict, "label")?);
        let (span, span_rows) = range_identity(u, object(dict.get("range")?)?, content)?;
        let matched_reference = reference(u, matched);
        let position = constant(u, position);
        let label = constant(u, label);
        let span_reference = reference(u, span);
        let capture_row = row(
            u,
            "source_capture",
            vec![matched_reference, position, label, span_reference],
        );
        rows.extend(span_rows);
        rows.push(located_row(u, source, span));
        rows.push(capture_row);
    }
    Some(rows)
}

/// `:213-226`.
fn replacement_rows(
    u: &mut Universe,
    replacement: &Value,
    matched: TermId,
    source: TermId,
    span: TermId,
) -> Option<Vec<TermId>> {
    if replacement.is_null() {
        return Some(vec![]);
    }
    let dict = object(replacement)?;
    let text = u.string(field_string(dict, "replacement")?);
    let producer = u.atom(field_string(dict, "producer")?);
    let occurrence = u.compound("located", vec![source, span]);
    let edit = u.compound("source_replacement", vec![matched, occurrence, producer]);
    let occurrence_reference = reference(u, occurrence);
    let source_reference = reference(u, source);
    let span_reference = reference(u, span);
    let edit_reference = reference(u, edit);
    let text = constant(u, text);
    let producer = constant(u, producer);
    Some(vec![
        row(
            u,
            "located",
            vec![occurrence_reference, source_reference, span_reference],
        ),
        row(
            u,
            "source_replacement",
            vec![edit_reference, occurrence_reference, text, producer],
        ),
    ])
}

/// `:228-237`.
fn range_identity(
    u: &mut Universe,
    dict: &Map<String, Value>,
    content: TermId,
) -> Option<(TermId, Vec<TermId>)> {
    let start = field_integer(dict, "start")?;
    let end = field_integer(dict, "end")?;
    if start > end {
        return None;
    }
    let start = u.int(start);
    let end = u.int(end);
    let span = u.compound("content_span", vec![content, start, end]);
    let span_reference = reference(u, span);
    let content_reference = reference(u, content);
    let start = constant(u, start);
    let end = constant(u, end);
    let rows = vec![row(
        u,
        "content_span",
        vec![span_reference, content_reference, start, end],
    )];
    Some((span, rows))
}

/// `:239-242`.
fn located_row(u: &mut Universe, source: TermId, span: TermId) -> TermId {
    let occurrence = u.compound("located", vec![source, span]);
    let occurrence_reference = reference(u, occurrence);
    let source_reference = reference(u, source);
    let span_reference = reference(u, span);
    row(
        u,
        "located",
        vec![occurrence_reference, source_reference, span_reference],
    )
}

/// `:261-281`. Sorted, typed terms so object-key order and scalar spelling
/// cannot change parse or query identity. A non-integer JSON number has no
/// term in this arena and voids the envelope; PLAN fork 6.
fn json_term(u: &mut Universe, value: &Value) -> Option<TermId> {
    match value {
        Value::String(text) => {
            let text = u.string(text);
            Some(u.compound("text", vec![text]))
        }
        Value::Number(number) => {
            let integer = number.as_i64()?;
            let integer = u.int(integer);
            Some(u.compound("integer", vec![integer]))
        }
        Value::Bool(flag) => {
            let flag = u.atom(if *flag { "true" } else { "false" });
            Some(u.compound("boolean", vec![flag]))
        }
        Value::Null => Some(u.atom("null")),
        Value::Array(items) => {
            let mut terms = Vec::with_capacity(items.len());
            for item in items {
                terms.push(json_term(u, item)?);
            }
            let list = u.list(&terms);
            Some(u.compound("array", vec![list]))
        }
        Value::Object(fields) => {
            let mut pairs = Vec::with_capacity(fields.len());
            for (key, item) in fields {
                let key = u.atom(key);
                let item = json_term(u, item)?;
                pairs.push(u.compound("-", vec![key, item]));
            }
            let pairs = sorted(u, pairs);
            let list = u.list(&pairs);
            Some(u.compound("object", vec![list]))
        }
    }
}

/// `:285-296`. Total: one basement, empty origins, no diagnostic.
pub fn install_source_fact_graph(
    u: &mut Universe,
    rows: &[TermId],
    basements: &[TermId],
    origins: &[TermId],
) -> Result<Installed, String> {
    let intelligence = u.atom("source_intelligence");
    let owner = u.compound("module", vec![intelligence]);

    let mut relations = Vec::with_capacity(SOURCE_RELATIONS.len());
    let mut edges = Vec::with_capacity(SOURCE_RELATIONS.len());
    for (index, (name, arity)) in SOURCE_RELATIONS.iter().enumerate() {
        let name_atom = u.atom(name);
        let callable = u.compound("source_relation", vec![name_atom]);
        let arity = u.int(*arity);
        let empty = u.empty_list();
        relations.push(u.compound("relation", vec![callable, arity, empty]));
        let target = u.compound("target", vec![callable]);
        let index = u.int(index as i64);
        edges.push(u.compound("pending_edge", vec![owner, name_atom, target, index]));
    }

    // :307-310. Every row must be a source_row/2 or the install fails.
    let mut seeds = Vec::with_capacity(rows.len());
    for term in rows {
        let parts = super::api::parts(u, *term, "source_row", 2)
            .ok_or("source fact row is not source_row/2")?;
        let head = u.compound("name", vec![owner, parts[0]]);
        seeds.push(u.compound("call", vec![head, parts[1]]));
    }

    let identity = u.compound("node", vec![owner]);
    let module_row = u.compound("module", vec![owner]);
    let product_row = u.compound("product", vec![owner]);
    let nodes = vec![identity, module_row, product_row];
    let basement = super::tsi::basement_program(u, &nodes, &edges, &relations, &seeds);
    let module_basement = u.compound("module_basement", vec![owner, basement]);
    let empty = u.empty_list();
    let module_origins = u.compound("module_origins", vec![owner, empty]);

    let mut out_basements = vec![module_basement];
    out_basements.extend_from_slice(basements);
    let mut out_origins = vec![module_origins];
    out_origins.extend_from_slice(origins);
    Ok(Installed {
        basements: out_basements,
        origins: out_origins,
        diagnostics: vec![],
    })
}
