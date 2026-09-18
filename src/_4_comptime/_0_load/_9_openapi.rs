//! OpenAPI 3.x documents as TSI wire rows. `oai.*` rows sit outside
//! `TSI_RELATIONS`; their relations are declared in `std/oai.dl7` and
//! `install_openapi_graph` only seeds them.

use super::api::{diagnostic, parts, sorted, Installed, Loaded};
use super::identity::{identity_map, Identities};
use super::tsi::{basement_program, install_tsi_graph, tsi_expression_environment};
use super::wire::{accepted_rows, facts_of, stream_owner, Owner};
use crate::_6_eval::term::{TermId, Universe};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The wire relation name, then the member label it seeds in `@std/oai`.
pub const OAI_RELATIONS: [(&str, &str); 2] = [("oai.route", "route"), ("oai.param", "param")];

const METHODS: [&str; 8] = [
    "get", "put", "post", "delete", "options", "head", "patch", "trace",
];

const SCHEMA_PREFIX: &str = "#/components/schemas/";

const RUN: i64 = 1;

/// Every document shares one run, so every schema lands under one owner and a
/// name two documents both declare is a collision.
pub fn openapi_rows(u: &mut Universe, documents: &[(TermId, Value)]) -> Loaded {
    let mut rows = Rows::new(documents);
    for index in 0..documents.len() {
        rows.check_version(u, index);
    }
    rows.index_schemas(u);
    rows.named_schemas(u);
    for index in 0..documents.len() {
        rows.routes(u, index);
    }
    rows.finish(u)
}

enum Arg<'a> {
    Id(i64),
    Text(&'a str),
    Int(i64),
    Atom(&'a str),
}

struct Rows<'d> {
    documents: &'d [(TermId, Value)],
    facts: Vec<TermId>,
    diagnostics: Vec<TermId>,
    next_id: i64,
    /// schema name -> the document that declares it, first document wins
    schemas: BTreeMap<String, usize>,
    /// schema name -> the id a `$ref` to it resolves to
    resolved: HashMap<String, i64>,
    /// set before the members are walked, so a recursive schema reaches itself
    nodes: HashMap<String, i64>,
    aliasing: HashSet<String>,
    primitives: BTreeMap<&'static str, i64>,
    optionals: HashMap<i64, i64>,
    arrays: HashMap<i64, i64>,
    array_head: Option<i64>,
}

impl<'d> Rows<'d> {
    fn new(documents: &'d [(TermId, Value)]) -> Self {
        Rows {
            documents,
            facts: Vec::new(),
            diagnostics: Vec::new(),
            next_id: 1,
            schemas: BTreeMap::new(),
            resolved: HashMap::new(),
            nodes: HashMap::new(),
            aliasing: HashSet::new(),
            primitives: BTreeMap::new(),
            optionals: HashMap::new(),
            arrays: HashMap::new(),
            array_head: None,
        }
    }

    fn finish(self, u: &mut Universe) -> Loaded {
        let mut rows = Vec::with_capacity(self.facts.len() * 2 + 2);
        let version = u.int(1);
        rows.push(u.compound("extract_protocol", vec![version]));
        let scope: Vec<TermId> = self.documents.iter().map(|(origin, _)| *origin).collect();
        let run = u.int(RUN);
        let mode = u.atom("semantic");
        let tool = u.atom("openapi");
        let tool_version = u.atom("3");
        let scope = u.list(&scope);
        rows.push(u.compound("extract_run", vec![run, mode, tool, tool_version, scope]));
        let method = u.atom("openapi");
        for (index, fact) in self.facts.iter().enumerate() {
            rows.push(*fact);
            let fact_number = u.int(index as i64 + 1);
            rows.push(u.compound("extract_witness", vec![fact_number, run, method]));
        }
        Loaded {
            rows,
            diagnostics: sorted(u, self.diagnostics),
        }
    }

    fn fact(&mut self, u: &mut Universe, relation: &str, arguments: &[Arg]) {
        let arguments: Vec<TermId> = arguments
            .iter()
            .map(|argument| match argument {
                Arg::Id(id) => {
                    let id = u.int(*id);
                    u.compound("id", vec![id])
                }
                Arg::Text(text) => {
                    let text = u.string(text);
                    u.compound("text", vec![text])
                }
                Arg::Int(number) => {
                    let number = u.int(*number);
                    u.compound("int", vec![number])
                }
                Arg::Atom(name) => {
                    let name = u.atom(name);
                    u.compound("atom", vec![name])
                }
            })
            .collect();
        let number = u.int(self.facts.len() as i64 + 1);
        let relation = u.atom(relation);
        let arguments = u.list(&arguments);
        self.facts
            .push(u.compound("extract_fact", vec![number, relation, arguments]));
    }

    fn fresh(&mut self) -> i64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn node(&mut self, u: &mut Universe) -> i64 {
        let id = self.fresh();
        self.fact(u, "tsi.type", &[Arg::Id(id)]);
        id
    }

    fn named_node(&mut self, u: &mut Universe, name: &str) -> i64 {
        let id = self.node(u);
        self.fact(u, "tsi.name", &[Arg::Id(id), Arg::Text(name)]);
        id
    }

    fn edge(&mut self, u: &mut Universe, owner: i64, label: &str, target: i64, position: usize) {
        let edge = self.fresh();
        self.fact(
            u,
            "tsi.edge",
            &[
                Arg::Id(edge),
                Arg::Id(owner),
                Arg::Text(label),
                Arg::Id(target),
                Arg::Int(position as i64),
            ],
        );
    }

    fn report(&mut self, u: &mut Universe, document: usize, payload: TermId) {
        let subject = u.compound("document", vec![self.documents[document].0]);
        self.diagnostics
            .push(diagnostic(u, "openapi", subject, payload));
    }

    fn report_text(&mut self, u: &mut Universe, document: usize, name: &str, texts: &[&str]) {
        let texts: Vec<TermId> = texts.iter().map(|text| u.string(text)).collect();
        let payload = u.compound(name, texts);
        self.report(u, document, payload);
    }

    fn check_version(&mut self, u: &mut Universe, document: usize) {
        let found = self.documents[document].1.get("openapi");
        if found
            .and_then(Value::as_str)
            .is_some_and(|v| v.starts_with("3."))
        {
            return;
        }
        let found = found
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".into());
        self.report_text(u, document, "openapi_version", &[&found]);
    }

    fn index_schemas(&mut self, u: &mut Universe) {
        for (index, (_, document)) in self.documents.iter().enumerate() {
            let Some(schemas) = document
                .pointer("/components/schemas")
                .and_then(Value::as_object)
            else {
                continue;
            };
            for name in schemas.keys() {
                if self.schemas.contains_key(name) {
                    self.report_text(u, index, "openapi_duplicate_schema", &[name]);
                } else {
                    self.schemas.insert(name.clone(), index);
                }
            }
        }
    }

    /// Name order, so a schema's ids do not move when the documents reorder.
    fn named_schemas(&mut self, u: &mut Universe) {
        let names: Vec<String> = self.schemas.keys().cloned().collect();
        for name in names {
            self.named(u, &name);
        }
    }

    fn named(&mut self, u: &mut Universe, name: &str) -> Option<i64> {
        if let Some(id) = self.resolved.get(name) {
            return Some(*id);
        }
        if let Some(id) = self.nodes.get(name) {
            return Some(*id);
        }
        let document = *self.schemas.get(name)?;
        let at = format!("{SCHEMA_PREFIX}{name}");
        let schema = self.documents[document].1.pointer(&at[1..])?;
        let id = if node_shaped(schema) {
            let node = self.named_node(u, name);
            self.nodes.insert(name.to_string(), node);
            self.fill_node(u, document, node, schema, name, &at)?;
            if nullable(schema) {
                self.optional(u, node)
            } else {
                node
            }
        } else {
            if !self.aliasing.insert(name.to_string()) {
                self.report_text(u, document, "openapi_alias_cycle", &[name]);
                return None;
            }
            let id = self.schema(u, document, schema, &at);
            self.aliasing.remove(name);
            id?
        };
        self.resolved.insert(name.to_string(), id);
        Some(id)
    }

    fn schema(
        &mut self,
        u: &mut Universe,
        document: usize,
        schema: &Value,
        at: &str,
    ) -> Option<i64> {
        if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
            return self.reference(u, document, reference);
        }
        let inner = if node_shaped(schema) {
            let node = self.node(u);
            self.fill_node(u, document, node, schema, "", at)?;
            node
        } else if schema.get("allOf").is_some() || schema.get("not").is_some() {
            let keyword = if schema.get("allOf").is_some() {
                "allOf"
            } else {
                "not"
            };
            self.report_text(u, document, "openapi_unmapped_schema", &[keyword, at]);
            return None;
        } else {
            match scalar_type(schema) {
                Ok(Some("array")) => {
                    let items = match schema.get("items") {
                        Some(items) => self.schema(u, document, items, &format!("{at}/items"))?,
                        None => self.primitive(u, "unknown"),
                    };
                    self.array(u, items)
                }
                Ok(Some(written)) => match primitive_class(written, schema) {
                    Some(class) => self.primitive(u, class),
                    None => {
                        self.report_text(u, document, "openapi_unmapped_schema", &[written, at]);
                        return None;
                    }
                },
                Ok(None) => self.primitive(u, "unknown"),
                Err(written) => {
                    self.report_text(u, document, "openapi_unmapped_schema", &[&written, at]);
                    return None;
                }
            }
        };
        Some(if nullable(schema) {
            self.optional(u, inner)
        } else {
            inner
        })
    }

    fn reference(&mut self, u: &mut Universe, document: usize, reference: &str) -> Option<i64> {
        if let Some(name) = reference.strip_prefix(SCHEMA_PREFIX) {
            if self.schemas.contains_key(name) {
                return self.named(u, name);
            }
        } else if let Some(target) = reference
            .strip_prefix('#')
            .and_then(|pointer| self.documents[document].1.pointer(pointer))
        {
            return self.schema(u, document, target, reference);
        }
        self.report_text(u, document, "openapi_unresolved_ref", &[reference]);
        None
    }

    /// A member that fails to map is reported and left out; the node stands.
    fn fill_node(
        &mut self,
        u: &mut Universe,
        document: usize,
        node: i64,
        schema: &Value,
        name: &str,
        at: &str,
    ) -> Option<()> {
        if let Some(values) = schema.get("enum").and_then(Value::as_array) {
            self.fact(u, "tsi.sum", &[Arg::Id(node)]);
            for (position, value) in values.iter().enumerate() {
                let Some(value) = value.as_str() else {
                    self.report_text(u, document, "openapi_unmapped_schema", &["enum", at]);
                    continue;
                };
                let written = if name.is_empty() {
                    value.to_string()
                } else {
                    format!("{name}::{value}")
                };
                let variant = self.named_node(u, &written);
                self.edge(u, node, value, variant, position);
            }
            return Some(());
        }
        for keyword in ["oneOf", "anyOf"] {
            let Some(branches) = schema.get(keyword).and_then(Value::as_array) else {
                continue;
            };
            self.fact(u, "tsi.sum", &[Arg::Id(node)]);
            let mapping = discriminator_labels(schema);
            for (position, branch) in branches.iter().enumerate() {
                let branch_at = format!("{at}/{keyword}/{position}");
                let Some(target) = self.schema(u, document, branch, &branch_at) else {
                    continue;
                };
                let label = branch_label(branch, &mapping, position);
                self.edge(u, node, &label, target, position);
            }
            return Some(());
        }
        self.fact(u, "tsi.product", &[Arg::Id(node)]);
        let required: HashSet<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
            return Some(());
        };
        for (position, (property, member)) in properties.iter().enumerate() {
            let member_at = format!("{at}/properties/{property}");
            let Some(target) = self.schema(u, document, member, &member_at) else {
                continue;
            };
            let target = if required.contains(property.as_str()) || nullable(member) {
                target
            } else {
                self.optional(u, target)
            };
            self.edge(u, node, property, target, position);
        }
        Some(())
    }

    fn primitive(&mut self, u: &mut Universe, class: &'static str) -> i64 {
        if let Some(id) = self.primitives.get(class) {
            return *id;
        }
        let id = self.node(u);
        self.fact(u, "tsi.primitive", &[Arg::Id(id), Arg::Atom(class)]);
        self.primitives.insert(class, id);
        id
    }

    /// The Kotlin `T?` shape: a sum of the value and the `null` class.
    fn optional(&mut self, u: &mut Universe, target: i64) -> i64 {
        if let Some(id) = self.optionals.get(&target) {
            return *id;
        }
        let null = self.primitive(u, "null");
        let id = self.node(u);
        self.fact(u, "tsi.sum", &[Arg::Id(id)]);
        self.edge(u, id, "value", target, 0);
        self.edge(u, id, "null", null, 1);
        self.optionals.insert(target, id);
        id
    }

    /// `Array<Items>`: an application of one named head, the way `Vec<T>`
    /// arrives from the Rust extractor.
    fn array(&mut self, u: &mut Universe, items: i64) -> i64 {
        if let Some(id) = self.arrays.get(&items) {
            return *id;
        }
        let head = match self.array_head {
            Some(head) => head,
            None => {
                let head = self.named_node(u, "Array");
                self.array_head = Some(head);
                head
            }
        };
        let id = self.node(u);
        let list = self.fresh();
        self.fact(
            u,
            "tsi.called",
            &[Arg::Id(id), Arg::Id(head), Arg::Id(list)],
        );
        self.fact(
            u,
            "tsi.argument",
            &[Arg::Id(list), Arg::Int(0), Arg::Id(items)],
        );
        self.arrays.insert(items, id);
        id
    }

    fn routes(&mut self, u: &mut Universe, document: usize) {
        let Some(paths) = self.documents[document]
            .1
            .get("paths")
            .and_then(Value::as_object)
        else {
            return;
        };
        for (path, item) in paths {
            let shared = item.get("parameters").and_then(Value::as_array);
            for method in METHODS {
                let Some(operation) = item.get(method) else {
                    continue;
                };
                self.operation(u, document, path, method, operation, shared);
            }
        }
    }

    fn operation(
        &mut self,
        u: &mut Universe,
        document: usize,
        path: &str,
        method: &str,
        operation: &Value,
        shared: Option<&Vec<Value>>,
    ) {
        let Some(operation_id) = operation.get("operationId").and_then(Value::as_str) else {
            self.report_text(u, document, "openapi_operation_without_id", &[path, method]);
            return;
        };
        let at = format!(
            "#/paths/{}/{method}",
            path.replace('~', "~0").replace('/', "~1")
        );
        let request = operation
            .get("requestBody")
            .and_then(|body| self.dereference(document, body))
            .and_then(json_schema);
        let request = match request {
            Some(schema) => self.schema(u, document, schema, &format!("{at}/requestBody")),
            None => Some(self.primitive(u, "void")),
        };
        let response = success_response(operation)
            .and_then(|response| self.dereference(document, response))
            .and_then(json_schema);
        let response = match response {
            Some(schema) => self.schema(u, document, schema, &format!("{at}/responses")),
            None => Some(self.primitive(u, "void")),
        };
        let (Some(request), Some(response)) = (request, response) else {
            return;
        };
        self.fact(
            u,
            "oai.route",
            &[
                Arg::Text(path),
                Arg::Atom(method),
                Arg::Text(operation_id),
                Arg::Id(request),
                Arg::Id(response),
            ],
        );

        let mut parameters: Vec<(String, String, &Value)> = Vec::new();
        let own = operation.get("parameters").and_then(Value::as_array);
        for parameter in shared.into_iter().chain(own).flatten() {
            let Some(parameter) = self.dereference(document, parameter) else {
                let reference = parameter.get("$ref").and_then(Value::as_str).unwrap_or("");
                self.report_text(u, document, "openapi_unresolved_ref", &[reference]);
                continue;
            };
            let (Some(name), Some(place)) = (
                parameter.get("name").and_then(Value::as_str),
                parameter.get("in").and_then(Value::as_str),
            ) else {
                continue;
            };
            parameters.retain(|(n, p, _)| !(n == name && p == place));
            parameters.push((name.to_string(), place.to_string(), parameter));
        }
        for (name, place, parameter) in parameters {
            let parameter_at = format!("{at}/parameters/{name}");
            let kind = match parameter.get("schema") {
                Some(schema) => self.schema(u, document, schema, &parameter_at),
                None => Some(self.primitive(u, "unknown")),
            };
            let Some(kind) = kind else {
                continue;
            };
            let required = parameter
                .get("required")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            self.fact(
                u,
                "oai.param",
                &[
                    Arg::Text(operation_id),
                    Arg::Text(&name),
                    Arg::Atom(&place),
                    Arg::Id(kind),
                    Arg::Atom(if required { "true" } else { "false" }),
                ],
            );
        }
    }

    /// A local `$ref` followed once; anything else is the value itself.
    fn dereference(&self, document: usize, value: &'d Value) -> Option<&'d Value> {
        match value.get("$ref").and_then(Value::as_str) {
            Some(reference) => self.documents[document]
                .1
                .pointer(reference.strip_prefix('#')?),
            None => Some(value),
        }
    }
}

fn node_shaped(schema: &Value) -> bool {
    if schema.get("$ref").is_some() {
        return false;
    }
    schema.get("enum").is_some()
        || schema.get("oneOf").is_some()
        || schema.get("anyOf").is_some()
        || schema.get("properties").is_some()
        || scalar_type(schema) == Ok(Some("object"))
}

/// 3.0 `nullable: true`, or 3.1 `type: [T, "null"]`.
fn nullable(schema: &Value) -> bool {
    schema.get("nullable").and_then(Value::as_bool) == Some(true)
        || schema
            .get("type")
            .and_then(Value::as_array)
            .is_some_and(|types| types.len() > 1 && types.iter().any(|t| t == "null"))
}

/// The one non-null `type`; several is `Err` with the written list.
fn scalar_type(schema: &Value) -> Result<Option<&str>, String> {
    match schema.get("type") {
        None => Ok(None),
        Some(Value::String(written)) => Ok(Some(written)),
        Some(Value::Array(types)) => {
            let named: Vec<&str> = types
                .iter()
                .filter_map(Value::as_str)
                .filter(|t| *t != "null")
                .collect();
            match named.as_slice() {
                [] => Ok(Some("null")),
                [one] => Ok(Some(one)),
                _ => Err(Value::Array(types.clone()).to_string()),
            }
        }
        Some(other) => Err(other.to_string()),
    }
}

/// `integer` has no class in `std/tsi.dl7` but a Rust width.
fn primitive_class(written: &str, schema: &Value) -> Option<&'static str> {
    let format = schema.get("format").and_then(Value::as_str);
    Some(match (written, format) {
        ("string", _) => "string",
        ("boolean", _) => "boolean",
        ("null", _) => "null",
        ("integer", Some("int32")) => "i32",
        ("integer", _) => "i64",
        ("number", Some("float")) => "f32",
        ("number", Some("double")) => "f64",
        ("number", _) => "number",
        _ => return None,
    })
}

fn json_schema(holder: &Value) -> Option<&Value> {
    let content = holder.get("content")?.as_object()?;
    let media = content
        .get("application/json")
        .or_else(|| content.values().next())?;
    media.get("schema")
}

/// The lowest `2XX` status in the document.
fn success_response(operation: &Value) -> Option<&Value> {
    let responses = operation.get("responses")?.as_object()?;
    let status = responses
        .keys()
        .filter(|status| status.starts_with('2'))
        .min()?;
    responses.get(status)
}

/// `discriminator.mapping` read backwards: `$ref` -> the tag that selects it.
fn discriminator_labels(schema: &Value) -> HashMap<String, String> {
    schema
        .pointer("/discriminator/mapping")
        .and_then(Value::as_object)
        .map(|mapping| {
            mapping
                .iter()
                .filter_map(|(tag, target)| Some((target.as_str()?.to_string(), tag.clone())))
                .collect()
        })
        .unwrap_or_default()
}

fn branch_label(branch: &Value, mapping: &HashMap<String, String>, position: usize) -> String {
    match branch.get("$ref").and_then(Value::as_str) {
        Some(reference) => mapping.get(reference).cloned().unwrap_or_else(|| {
            reference
                .rsplit('/')
                .next()
                .unwrap_or(reference)
                .to_string()
        }),
        None => match scalar_type(branch) {
            Ok(Some(written)) => written.to_string(),
            _ => position.to_string(),
        },
    }
}

/// The `@std/oai` member a wire relation seeds.
fn oai_member(name: &str) -> Option<&'static str> {
    OAI_RELATIONS
        .iter()
        .find(|(relation, _)| *relation == name)
        .map(|(_, member)| *member)
}

fn route_fact(u: &Universe, row: TermId) -> bool {
    parts(u, row, "extract_fact", 3)
        .and_then(|args| super::api::atom_text(u, args[1]).map(|name| oai_member(name).is_some()))
        .unwrap_or(false)
}

/// The module `@std/oai` declares, and the owner its members hang off.
pub fn oai_module_owner(u: &mut Universe) -> TermId {
    let name = u.atom("oai");
    let origin = u.compound("std", vec![name]);
    u.compound("module", vec![origin])
}

/// `install_tsi_graph` over the type rows, then one seed per route and
/// parameter against the relation `std/oai.dl7` declares.
pub fn install_openapi_graph(
    u: &mut Universe,
    rows: &[TermId],
    basements: &[TermId],
    origins: &[TermId],
) -> Installed {
    let (routes, types): (Vec<TermId>, Vec<TermId>) =
        rows.iter().partition(|row| route_fact(u, **row));
    let installed = install_tsi_graph(u, &types, basements, origins);
    let Owner::Owner(owner) = stream_owner(u, &types) else {
        return installed;
    };
    if !installed.diagnostics.is_empty() {
        return installed;
    }
    let accepted_terms = accepted_rows(u, &types);
    let accepted = facts_of(u, &accepted_terms);
    let (identities, _) = identity_map(u, owner, &accepted, basements);
    let oai = oai_module_owner(u);

    let mut seeds = Vec::new();
    for route in &routes {
        let args = parts(u, *route, "extract_fact", 3).expect("route shape");
        let Some(member) = super::api::atom_text(u, args[1]).and_then(oai_member) else {
            continue;
        };
        let arguments = u.as_list(args[2]).unwrap_or_default();
        let arguments: Option<Vec<TermId>> = arguments
            .iter()
            .map(|argument| seed_argument(u, &identities, *argument))
            .collect();
        let Some(arguments) = arguments else {
            continue;
        };
        let label = u.atom(member);
        let head = u.compound("name", vec![oai, label]);
        let argument_list = u.list(&arguments);
        seeds.push(u.compound("call", vec![head, argument_list]));
    }
    let seeds = sorted(u, seeds);

    let mut basements_out = installed.basements;
    let Some([basement_owner, program]) = u.args::<2>(basements_out[0], "module_basement") else {
        return Installed {
            basements: basements_out,
            origins: installed.origins,
            diagnostics: installed.diagnostics,
        };
    };
    debug_assert_eq!(basement_owner, owner);
    let [graph, datalog] = u
        .args::<2>(program, "basement_program")
        .expect("basement shape");
    let [nodes, edges] = u.args::<2>(graph, "root_graph").expect("graph shape");
    let [relations, old_seeds, _] = u
        .args::<3>(datalog, "datalog_program")
        .expect("datalog shape");
    let nodes = u.as_list(nodes).unwrap_or_default();
    let edges = u.as_list(edges).unwrap_or_default();
    let relations = u.as_list(relations).unwrap_or_default();
    let mut all_seeds = u.as_list(old_seeds).unwrap_or_default();
    all_seeds.extend(seeds);

    let program = basement_program(u, &nodes, &edges, &relations, &all_seeds);
    basements_out[0] = u.compound("module_basement", vec![owner, program]);
    Installed {
        basements: basements_out,
        origins: installed.origins,
        diagnostics: installed.diagnostics,
    }
}

fn seed_argument(u: &mut Universe, identities: &Identities, argument: TermId) -> Option<TermId> {
    if let Some(id) = super::api::wire_id(u, argument) {
        let identity = identities.of(id)?;
        return Some(u.compound("ref", vec![identity]));
    }
    let (name, args) = u.functor(argument)?;
    match (name, args.len()) {
        ("text" | "int" | "atom", 1) => {
            let value = args[0];
            Some(u.compound("const", vec![value]))
        }
        _ => None,
    }
}

/// The type rows only: the `oai.*` relations come from `@std/oai`, imported.
pub fn openapi_expression_environment(
    u: &mut Universe,
    rows: &[TermId],
    importers: &[TermId],
) -> TermId {
    let types: Vec<TermId> = rows
        .iter()
        .copied()
        .filter(|row| !route_fact(u, *row))
        .collect();
    tsi_expression_environment(u, &types, importers)
}
