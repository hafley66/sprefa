//! The ts syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.ts` under `--witness --family type`,
//! the twin of `tests/106_rust_syntax_graph.rs`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `1696ce96a`
//! the fixture is absent; with it restored and `ts.rs` at that sha, every
//! case below fails. Per defect, the row the base sha lacks:
//!
//! - T1: `tsi.called` fires only from an alias body; a field written
//!   `Map<string, Option<T>>` is one bare id, a tuple field has no
//!   `tsi.product`, a literal field no edges, a function-typed field no
//!   `tsi.callable`, and a reference origin spans the whole text.
//! - T2: zero `tsi.has_type`; `const` and `let` annotations are skipped.
//! - T3: zero `tsi.primitive`; `string`, `number`, `boolean`, `void` mint
//!   `tsi.type` + `tsi.origin` at their reference span.

use std::collections::BTreeMap;
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/tsi/probe_graph.ts";

struct Probe {
    facts: Vec<FactOut>,
    source: Vec<u8>,
}

impl Probe {
    fn read() -> Self {
        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--witness", "--family", "type", FIXTURE])
            .output()
            .expect("extract binary runs");
        assert!(
            output.status.success(),
            "stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stream = String::from_utf8(output.stdout).expect("stdout is UTF-8");
        let mut facts = Vec::new();
        for line in stream.lines() {
            let row: FlatFact = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"));
            if let FlatFact::Fact(fact) = row {
                facts.push(fact);
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE);
        let source = std::fs::read(&path).expect("the fixture is readable");
        Self { facts, source }
    }

    fn rows(&self, wanted: &str) -> Vec<&FactOut> {
        self.facts
            .iter()
            .filter(|fact| fact.relation == wanted)
            .collect()
    }

    fn slice(&self, start: u32, end: u32) -> String {
        String::from_utf8_lossy(&self.source[start as usize..end as usize]).to_string()
    }

    fn origin(&self, id: u32) -> (u32, u32) {
        let mut found: Vec<(u32, u32)> = self
            .rows("tsi.origin")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(id))
            .filter_map(|fact| as_span(&fact.args[2]))
            .collect();
        assert_eq!(found.len(), 1, "id {id} has {} origin rows", found.len());
        found.remove(0)
    }

    fn origin_text(&self, id: u32) -> String {
        let (start, end) = self.origin(id);
        self.slice(start, end)
    }

    /// id -> `tsi.name` text.
    fn names(&self) -> BTreeMap<u32, String> {
        self.rows("tsi.name")
            .into_iter()
            .filter_map(|fact| Some((as_id(&fact.args[0])?, as_text(&fact.args[1])?.to_string())))
            .collect()
    }

    /// The one id whose `tsi.name` is `name`.
    fn id_named(&self, name: &str) -> u32 {
        let found: Vec<u32> = self
            .names()
            .into_iter()
            .filter(|(_, text)| text == name)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(found.len(), 1, "`{name}` names {} ids", found.len());
        found[0]
    }

    fn edge(&self, owner: u32, label: &str) -> &FactOut {
        let found: Vec<&FactOut> = self
            .rows("tsi.edge")
            .into_iter()
            .filter(|fact| as_id(&fact.args[1]) == Some(owner) && as_text(&fact.args[2]) == Some(label))
            .collect();
        assert_eq!(found.len(), 1, "owner {owner} has {} `{label}` edges", found.len());
        found[0]
    }

    fn edges_of(&self, owner: u32) -> Vec<&FactOut> {
        self.rows("tsi.edge")
            .into_iter()
            .filter(|fact| as_id(&fact.args[1]) == Some(owner))
            .collect()
    }

    fn carries(&self, wanted: &str, id: u32) -> bool {
        self.rows(wanted)
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
    }

    fn call_of(&self, result: u32) -> Option<(u32, Vec<u32>)> {
        let row = self
            .rows("tsi.called")
            .into_iter()
            .find(|fact| as_id(&fact.args[0]) == Some(result))?;
        let callee = as_id(&row.args[1])?;
        let list = as_id(&row.args[2])?;
        let mut arguments: Vec<(i64, u32)> = self
            .rows("tsi.argument")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(list))
            .filter_map(|fact| Some((as_int(&fact.args[1])?, as_id(&fact.args[2])?)))
            .collect();
        arguments.sort_unstable();
        Some((callee, arguments.into_iter().map(|(_, id)| id).collect()))
    }
}

fn as_id(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Id(id) => Some(*id),
        _ => None,
    }
}

fn as_text(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Text(text) => Some(text),
        _ => None,
    }
}

fn as_atom(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Atom(atom) => Some(atom),
        _ => None,
    }
}

fn as_int(arg: &Arg) -> Option<i64> {
    match arg {
        Arg::Int(int) => Some(*int),
        _ => None,
    }
}

fn as_span(arg: &Arg) -> Option<(u32, u32)> {
    match arg {
        Arg::Span(_, start, end) => Some((*start, *end)),
        _ => None,
    }
}

/// T1: a field written `Map<string, Option<T>>` is a call on `Map` whose
/// second argument is itself a call on `Option` with the scoped `T`.
#[test]
fn every_written_application_states_its_call() {
    let probe = Probe::read();
    let trail = probe.id_named("Trail");
    let steps = as_id(&probe.edge(trail, "steps").args[3]).unwrap();
    assert_eq!(probe.origin_text(steps), "Map");
    let (map, arguments) = probe.call_of(steps).expect("steps is a call");
    assert_eq!(probe.names()[&map], "Map");
    assert_eq!(arguments.len(), 2);
    assert_eq!(as_atom(&probe.rows("tsi.primitive").iter().find(|f| as_id(&f.args[0]) == Some(arguments[0])).unwrap().args[1]), Some("string"));
    let (option, inner) = probe.call_of(arguments[1]).expect("Option<T> is a call");
    assert_eq!(probe.names()[&option], "Option");
    let parameter = probe.rows("tsi.parameter");
    let scoped: Vec<u32> = parameter
        .iter()
        .filter(|fact| as_id(&fact.args[1]) == Some(trail))
        .filter_map(|fact| as_id(&fact.args[0]))
        .collect();
    assert_eq!(inner, scoped, "the argument is the class's own T");

    let outcome = as_id(&probe.edge(trail, "outcome").args[3]).unwrap();
    let (result, arguments) = probe.call_of(outcome).expect("outcome is a call");
    assert_eq!(probe.names()[&result], "Result");
    assert_eq!(probe.names()[&arguments[1]], "Error");
    assert_eq!(probe.call_of(probe.id_named("Query")).map(|(callee, _)| probe.names()[&callee].clone()), Some("Partial".to_string()));
}

/// T1: a tuple field is an anonymous product with positional edges.
#[test]
fn a_tuple_is_an_anonymous_product() {
    let probe = Probe::read();
    let trail = probe.id_named("Trail");
    let label = as_id(&probe.edge(trail, "label").args[3]).unwrap();
    assert!(probe.carries("tsi.product", label));
    assert!(!probe.names().contains_key(&label), "a tuple has no name");
    let edges = probe.edges_of(label);
    let labels: Vec<&str> = edges.iter().filter_map(|f| as_text(&f.args[2])).collect();
    assert_eq!(labels, ["0", "1"]);
    assert_eq!(probe.names()[&as_id(&edges[1].args[3]).unwrap()], "number");
}

/// T1: a literal field is an anonymous product with labelled edges and the
/// optional marker on the optional one.
#[test]
fn a_literal_is_an_anonymous_product_with_labels() {
    let probe = Probe::read();
    let trail = probe.id_named("Trail");
    let failed = as_id(&probe.edge(trail, "failed").args[3]).unwrap();
    assert!(probe.carries("tsi.product", failed));
    let code = probe.edge(failed, "code");
    assert!(probe.carries("ts.optional", as_id(&code.args[0]).unwrap()));
    assert_eq!(probe.names()[&as_id(&probe.edge(failed, "reason").args[3]).unwrap()], "string");
}

/// T1: a function-typed field is an anonymous callable with its inputs and
/// output, the input being the class's scoped `T`.
#[test]
fn a_function_type_is_an_anonymous_callable() {
    let probe = Probe::read();
    let trail = probe.id_named("Trail");
    let project = as_id(&probe.edge(trail, "project").args[3]).unwrap();
    assert!(probe.carries("tsi.callable", project));
    let inputs: Vec<&FactOut> = probe
        .rows("tsi.input")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(project))
        .collect();
    assert_eq!(inputs.len(), 1);
    assert_eq!(probe.names()[&as_id(&inputs[0].args[2]).unwrap()], "T");
    let outputs: Vec<&FactOut> = probe
        .rows("tsi.output")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(project))
        .collect();
    assert_eq!(probe.names()[&as_id(&outputs[0].args[2]).unwrap()], "U");
}

/// T2: `const` and `let` annotations are occurrences with a type.
#[test]
fn a_typed_binding_has_a_type() {
    let probe = Probe::read();
    let mut found: Vec<(String, String)> = probe
        .rows("tsi.has_type")
        .into_iter()
        .filter_map(|fact| {
            let (start, end) = as_span(&fact.args[0])?;
            Some((probe.slice(start, end), probe.names()[&as_id(&fact.args[1])?].clone()))
        })
        .collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("RETRY_LIMIT".to_string(), "number".to_string()),
            ("banner".to_string(), "string".to_string()),
        ]
    );
}

/// T3: keyword types are primitives with a class, a name and no origin.
#[test]
fn keywords_are_primitives_without_origin() {
    let probe = Probe::read();
    let mut classes: Vec<(u32, String)> = probe
        .rows("tsi.primitive")
        .into_iter()
        .filter_map(|fact| Some((as_id(&fact.args[0])?, as_atom(&fact.args[1])?.to_string())))
        .collect();
    classes.sort();
    let spelled: Vec<&str> = classes.iter().map(|(_, class)| class.as_str()).collect();
    assert_eq!(spelled, ["string", "number", "boolean", "void"]);
    let names = probe.names();
    for (id, class) in &classes {
        assert_eq!(names.get(id), Some(class), "primitive {id} names its class");
        assert!(
            !probe.carries("tsi.origin", *id),
            "primitive {class} carries an origin"
        );
    }
}

/// Every `tsi.type` id carries a `tsi.name`, or is a tuple, literal or
/// function type (anonymous, rule 2).
#[test]
fn every_named_id_spells_its_text() {
    let probe = Probe::read();
    let names = probe.names();
    for fact in probe.rows("tsi.type") {
        let id = as_id(&fact.args[0]).unwrap();
        if names.contains_key(&id) {
            continue;
        }
        let origin = probe.origin_text(id);
        assert!(
            origin.starts_with('[') || origin.starts_with('{') || origin.starts_with('('),
            "id {id} written `{origin}` has no tsi.name"
        );
    }
}
