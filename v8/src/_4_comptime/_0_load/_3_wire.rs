//! The TSI wire: lines to rows, then rows to accepted rows and a stream owner.
//! Port of `v7/src/2_comptime/0c_extract_loader.pl:74-356` and `:396-410`.

use super::api::{atom_text, diagnostic, parts, sorted, wire_id, Loaded};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// `:15-50`, the relation table of `v6/sprefa-extract/src/tsi/registry.rs`.
pub const TSI_RELATIONS: [(&str, i64); 36] = [
    ("tsi.type", 1),
    ("tsi.name", 2),
    ("tsi.symbol", 1),
    ("tsi.denotes", 2),
    ("tsi.scip_symbol", 2),
    ("tsi.value", 2),
    ("tsi.value_argument", 3),
    ("tsi.has_type", 2),
    ("tsi.origin", 3),
    ("tsi.product", 1),
    ("tsi.sum", 1),
    ("tsi.callable", 1),
    ("tsi.primitive", 2),
    ("tsi.edge", 5),
    ("tsi.parameter", 4),
    ("tsi.called", 3),
    ("tsi.argument", 3),
    ("tsi.input", 3),
    ("tsi.output", 3),
    ("tsi.subtype", 3),
    ("tsi.assignable", 3),
    ("tsi.conforms", 3),
    ("tsi.equivalent", 3),
    ("ts.interface", 1),
    ("ts.conditional", 5),
    ("ts.mapped", 4),
    ("ts.readonly", 1),
    ("ts.optional", 1),
    ("rust.trait", 1),
    ("rust.impl", 3),
    ("rust.lifetime", 2),
    ("rust.ownership", 2),
    ("rust.assoc", 3),
    ("go.interface", 1),
    ("go.type_set", 2),
    ("go.embedding", 2),
];

/// `:54-64`. These reach the basement as graph rows, never as relations.
pub const GRAPH_RELATIONS: [&str; 11] = [
    "tsi.type",
    "tsi.product",
    "tsi.sum",
    "tsi.edge",
    "tsi.primitive",
    "tsi.symbol",
    "tsi.value",
    "tsi.value_argument",
    "tsi.called",
    "tsi.argument",
    "tsi.origin",
];

/// `:145-200`. `types.rs:2976` FlatFact less the TSI six, plus the astgrep and
/// ast-rule records. A name here is skipped without a diagnostic.
pub const FOREIGN_RECORDS: [&str; 56] = [
    "arg",
    "ast_rule",
    "capture",
    "cfg_scope",
    "const",
    "data_doc",
    "data_value",
    "df_allocates",
    "df_field",
    "df_lit",
    "df_loop",
    "df_nest",
    "doc",
    "doc_node",
    "doc_tag",
    "edge",
    "file",
    "file_edge",
    "file_unresolved",
    "flow_edge",
    "macro_site",
    "method_owner",
    "node",
    "package_edge",
    "param",
    "projectedge",
    "reference",
    "resolved_edge",
    "resolved_import",
    "resolved_type_edge",
    "scip_callee_type",
    "scip_def",
    "scip_diagnostic",
    "scip_document",
    "scip_documentation",
    "scip_edge",
    "scip_fn_edge",
    "scip_impl",
    "scip_index",
    "scip_local",
    "scip_metadata",
    "scip_name",
    "scip_occurrence",
    "scip_occurrence_doc",
    "scip_ref",
    "scip_relationship",
    "scip_signature",
    "scip_signature_occurrence",
    "scip_skip",
    "scip_symbol",
    "sig",
    "site",
    "size_skip",
    "specifier",
    "test_only_call",
    "unresolved",
];

pub fn tsi_relation_arity(name: &str) -> Option<i64> {
    TSI_RELATIONS
        .iter()
        .find(|(relation, _)| *relation == name)
        .map(|(_, arity)| *arity)
}

/// `:68`. The cap bounds a stream whose application ids never settle.
pub const IDENTITY_PASSES: u32 = 16;

/// `:74` and `:83` share every line of their bodies; only the origin term
/// differs, and it is the one the diagnostics carry.
pub fn load_tsi_lines(u: &mut Universe, origin: TermId, lines: &[String]) -> Loaded {
    let mut rows = Vec::new();
    let mut diagnostics = Vec::new();
    for (offset, line) in lines.iter().enumerate() {
        let number = offset as i64 + 1;
        if line.chars().all(char::is_whitespace) {
            continue;
        }
        match parse_one(line) {
            Some(value) => match decode_line(u, &value) {
                Decoded::Row(row) => rows.push(row),
                Decoded::Skip => {}
                Decoded::Malformed(record) => {
                    let name = u.atom(&record);
                    let detail = u.compound("malformed_record", vec![name]);
                    diagnostics.push(line_diagnostic(u, origin, number, detail));
                }
                Decoded::NoRecordKey => {
                    let detail = u.atom("no_record_key");
                    diagnostics.push(line_diagnostic(u, origin, number, detail));
                }
            },
            None => {
                let detail = u.atom("not_json");
                diagnostics.push(line_diagnostic(u, origin, number, detail));
            }
        }
    }
    finish_stream_rows(u, origin, rows, diagnostics)
}

fn line_diagnostic(u: &mut Universe, origin: TermId, number: i64, detail: TermId) -> TermId {
    let number_term = u.int(number);
    let payload = u.compound("tsi_line", vec![origin, number_term, detail]);
    let subject = u.compound("stream", vec![origin]);
    diagnostic(u, "extract", subject, payload)
}

/// `:91-99`. One bad protocol voids the whole stream.
fn finish_stream_rows(
    u: &mut Universe,
    origin: TermId,
    rows: Vec<TermId>,
    diagnostics: Vec<TermId>,
) -> Loaded {
    for row in &rows {
        if let Some(version) = u.unary(*row, "extract_protocol") {
            if u.as_int(version) != Some(1) {
                let payload = u.compound("tsi_protocol", vec![version]);
                let subject = u.compound("stream", vec![origin]);
                let row = diagnostic(u, "extract", subject, payload);
                return Loaded {
                    rows: vec![],
                    diagnostics: vec![row],
                };
            }
        }
    }
    Loaded { rows, diagnostics }
}

enum Decoded {
    Row(TermId),
    Skip,
    Malformed(String),
    NoRecordKey,
}

/// SWI's `json_read` takes one value and leaves any trailing text, verified on
/// swipl 10.0.2 with `{"a":1} trailing`.
fn parse_one(line: &str) -> Option<Value> {
    let mut stream = serde_json::Deserializer::from_str(line).into_iter::<Value>();
    stream.next()?.ok()
}

fn decode_line(u: &mut Universe, value: &Value) -> Decoded {
    let Some(object) = value.as_object() else {
        return Decoded::NoRecordKey;
    };
    let Some(Value::String(record)) = object.get("record") else {
        return Decoded::NoRecordKey;
    };
    match decode_record(u, record, object) {
        Some(row) => Decoded::Row(row),
        None if FOREIGN_RECORDS.contains(&record.as_str()) => Decoded::Skip,
        None => Decoded::Malformed(record.clone()),
    }
}

type Object = serde_json::Map<String, Value>;

fn integer(object: &Object, key: &str) -> Option<i64> {
    object.get(key)?.as_i64()
}

fn text<'a>(object: &'a Object, key: &str) -> Option<&'a str> {
    match object.get(key)? {
        Value::String(s) => Some(s),
        _ => None,
    }
}

/// `:214-250`.
fn decode_record(u: &mut Universe, record: &str, object: &Object) -> Option<TermId> {
    match record {
        "protocol" => {
            let version = u.int(integer(object, "version")?);
            Some(u.compound("extract_protocol", vec![version]))
        }
        "run" => {
            let run = integer(object, "run")?;
            let mode = text(object, "mode")?;
            if mode != "syntax" && mode != "semantic" {
                return None;
            }
            let tool = text(object, "tool")?.to_string();
            let version = text(object, "version")?.to_string();
            let scope_values = object.get("scope")?.as_array()?;
            let mut scope = Vec::with_capacity(scope_values.len());
            for value in scope_values {
                let Value::String(digest) = value else {
                    return None;
                };
                scope.push(u.atom(digest));
            }
            let run = u.int(run);
            let mode = u.atom(mode);
            let tool = u.atom(&tool);
            let version = u.atom(&version);
            let scope = u.list(&scope);
            Some(u.compound("extract_run", vec![run, mode, tool, version, scope]))
        }
        "fact" => {
            let fact = integer(object, "fact")?;
            let relation = text(object, "relation")?.to_string();
            let argument_values = object.get("args")?.as_array()?;
            let mut arguments = Vec::with_capacity(argument_values.len());
            for value in argument_values {
                arguments.push(decode_argument(u, value)?);
            }
            let fact = u.int(fact);
            let relation = u.atom(&relation);
            let arguments = u.list(&arguments);
            Some(u.compound("extract_fact", vec![fact, relation, arguments]))
        }
        "witness" => {
            let fact = integer(object, "fact")?;
            let run = integer(object, "run")?;
            let method = text(object, "method")?.to_string();
            let fact = u.int(fact);
            let run = u.int(run);
            let method = u.atom(&method);
            Some(u.compound("extract_witness", vec![fact, run, method]))
        }
        "coverage" => {
            let run = integer(object, "run")?;
            let relation = text(object, "relation")?.to_string();
            let coverage = text(object, "coverage")?;
            if coverage != "partial" && coverage != "complete" {
                return None;
            }
            let run = u.int(run);
            let relation = u.atom(&relation);
            let coverage = u.atom(coverage);
            Some(u.compound("extract_coverage", vec![run, relation, coverage]))
        }
        "diagnostic" => {
            let run = integer(object, "run")?;
            let relation = text(object, "relation")?.to_string();
            let detail = text(object, "detail")?.to_string();
            let run = u.int(run);
            let relation = u.atom(&relation);
            let detail = u.atom(&detail);
            Some(u.compound("extract_diagnostic", vec![run, relation, detail]))
        }
        _ => None,
    }
}

/// `:261-283`. The shapes are tried in this order and the first that fits wins.
fn decode_argument(u: &mut Universe, value: &Value) -> Option<TermId> {
    let object = value.as_object()?;
    if let Some(id) = integer(object, "id") {
        let id = u.int(id);
        return Some(u.compound("id", vec![id]));
    }
    if let Some(Value::Array(span)) = object.get("span") {
        if span.len() == 3 {
            if let (Value::String(digest), Some(start), Some(end)) =
                (&span[0], span[1].as_i64(), span[2].as_i64())
            {
                let digest = u.atom(digest);
                let start = u.int(start);
                let end = u.int(end);
                return Some(u.compound("span", vec![digest, start, end]));
            }
        }
    }
    if let Some(body) = text(object, "text") {
        let body = u.string(body);
        return Some(u.compound("text", vec![body]));
    }
    if let Some(number) = integer(object, "int") {
        let number = u.int(number);
        return Some(u.compound("int", vec![number]));
    }
    if let Some(name) = text(object, "atom") {
        let name = u.atom(name);
        return Some(u.compound("atom", vec![name]));
    }
    None
}

/// One decoded `extract_fact(Fact, Relation, Arguments)` row, split so the
/// acceptance and identity passes never re-walk the term.
pub struct Fact {
    pub term: TermId,
    pub fact: i64,
    pub relation: String,
    pub arguments: Vec<TermId>,
}

pub fn facts_of(u: &Universe, rows: &[TermId]) -> Vec<Fact> {
    let mut out = Vec::new();
    for row in rows {
        let Some(args) = parts(u, *row, "extract_fact", 3) else {
            continue;
        };
        let (Some(fact), Some(relation), Some(arguments)) = (
            u.as_int(args[0]),
            atom_text(u, args[1]).map(|t| t.to_string()),
            u.as_list(args[2]),
        ) else {
            continue;
        };
        out.push(Fact {
            term: *row,
            fact,
            relation,
            arguments,
        });
    }
    out
}

struct Runs {
    /// run -> (mode, scope term)
    by_run: BTreeMap<i64, (String, TermId)>,
    /// (scope term, relation) with a complete claim -> the runs that claim it
    complete: HashMap<(u32, String), BTreeSet<i64>>,
    witnesses: BTreeMap<i64, Vec<i64>>,
}

fn index_runs(u: &Universe, rows: &[TermId]) -> Runs {
    let mut by_run = BTreeMap::new();
    let mut coverage: Vec<(i64, String)> = Vec::new();
    let mut witnesses: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for row in rows {
        if let Some(args) = parts(u, *row, "extract_run", 5) {
            if let (Some(run), Some(mode)) = (u.as_int(args[0]), atom_text(u, args[1])) {
                by_run
                    .entry(run)
                    .or_insert_with(|| (mode.to_string(), args[4]));
            }
        }
        if let Some(args) = parts(u, *row, "extract_coverage", 3) {
            if let (Some(run), Some(relation), Some("complete")) = (
                u.as_int(args[0]),
                atom_text(u, args[1]).map(|t| t.to_string()),
                atom_text(u, args[2]),
            ) {
                coverage.push((run, relation));
            }
        }
        if let Some(args) = parts(u, *row, "extract_witness", 3) {
            if let (Some(fact), Some(run)) = (u.as_int(args[0]), u.as_int(args[1])) {
                witnesses.entry(fact).or_default().push(run);
            }
        }
    }
    let mut complete: HashMap<(u32, String), BTreeSet<i64>> = HashMap::new();
    for (run, relation) in coverage {
        if let Some((mode, scope)) = by_run.get(&run) {
            if mode == "semantic" {
                complete.entry((scope.0, relation)).or_default().insert(run);
            }
        }
    }
    Runs {
        by_run,
        complete,
        witnesses,
    }
}

/// `:289-307`. A semantic witness on the newest complete run wins; failing
/// that, a syntax witness with no complete semantic claim at all.
pub fn accepted_rows(u: &Universe, rows: &[TermId]) -> Vec<TermId> {
    let runs = index_runs(u, rows);
    let mut accepted = Vec::new();
    for fact in facts_of(u, rows) {
        if accepted_fact(&runs, fact.fact, &fact.relation) {
            accepted.push(fact.term);
        }
    }
    sorted(u, accepted)
}

fn accepted_fact(runs: &Runs, fact: i64, relation: &str) -> bool {
    let Some(witness_runs) = runs.witnesses.get(&fact) else {
        return false;
    };
    for run in witness_runs {
        let Some((mode, scope)) = runs.by_run.get(run) else {
            continue;
        };
        if mode != "semantic" {
            continue;
        }
        // :340-345. With no complete claim every semantic run stands.
        match runs.complete.get(&(scope.0, relation.to_string())) {
            None => return true,
            Some(complete) => {
                if complete.iter().next_back() == Some(run) {
                    return true;
                }
            }
        }
    }
    for run in witness_runs {
        let Some((mode, scope)) = runs.by_run.get(run) else {
            continue;
        };
        if mode != "syntax" {
            continue;
        }
        if !runs.complete.contains_key(&(scope.0, relation.to_string())) {
            return true;
        }
    }
    false
}

pub enum Owner {
    None,
    MissingRun,
    Owner(TermId),
}

/// `:398-410`. The highest run number names the owner.
pub fn stream_owner(u: &mut Universe, rows: &[TermId]) -> Owner {
    let mut claims: Vec<TermId> = Vec::new();
    for row in rows {
        if let Some(args) = parts(u, *row, "extract_run", 5) {
            let owner = u.compound("owner", vec![args[2], args[4]]);
            claims.push(u.compound("-", vec![args[0], owner]));
        }
    }
    if claims.is_empty() {
        return if rows.is_empty() {
            Owner::None
        } else {
            Owner::MissingRun
        };
    }
    let claims = sorted(u, claims);
    let last = *claims.last().expect("non-empty");
    let pair = parts(u, last, "-", 2).expect("run pair");
    let owner = parts(u, pair[1], "owner", 2).expect("owner pair");
    let tsi = u.compound("tsi", vec![owner[0], owner[1]]);
    Owner::Owner(u.compound("module", vec![tsi]))
}

/// `:691-704`. Registry relations that are not graph relations, plus a
/// diagnostic for every relation outside the registry.
pub fn relation_names(u: &mut Universe, accepted: &[Fact]) -> (Vec<String>, Vec<TermId>) {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut unknown: BTreeSet<String> = BTreeSet::new();
    for fact in accepted {
        if tsi_relation_arity(&fact.relation).is_some() {
            if !GRAPH_RELATIONS.contains(&fact.relation.as_str()) {
                names.insert(fact.relation.clone());
            }
        } else {
            unknown.insert(fact.relation.clone());
        }
    }
    let mut diagnostics = Vec::new();
    for name in unknown {
        let name = u.atom(&name);
        let payload = u.compound("tsi_unknown_relation", vec![name]);
        let none = u.atom("none");
        diagnostics.push(diagnostic(u, "extract", none, payload));
    }
    let names: Vec<String> = names.into_iter().collect();
    let diagnostics = sorted(u, diagnostics);
    (names, diagnostics)
}

/// `id(Id)` arguments of a fact, in wire order.
pub fn argument_ids(u: &Universe, arguments: &[TermId]) -> Vec<i64> {
    arguments.iter().filter_map(|a| wire_id(u, *a)).collect()
}
