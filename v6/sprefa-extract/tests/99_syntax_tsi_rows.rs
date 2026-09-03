//! The syntax tier's TSI rows for ts and rust: what a parse alone can say
//! about a type, under `--witness`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `8e050ed82`
//! `extract --witness --family type tests/fixtures/tsi/probe.ts` emits zero
//! `record=fact` rows (A3 landed the sink and the registry; nothing wrote
//! through them), and both fixtures are absent, so every case below sees an
//! empty fact set. `flag_off_emits_no_fact_row` is the one guarding the other
//! direction; `cargo test --test golden_parity` is its whole-corpus twin.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::process::{Command, Stdio};

use sprefa_extract::tsi::{relation, Arg, FactOut};
use sprefa_extract::FlatFact;

const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_PROBE: &str = "tests/fixtures/tsi/probe.rs";
const GO_PROBE: &str = "tests/fixtures/tsi/probe_graph.go";

fn extract(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// One witnessed `--family type` run over one fixture, with the fixture's own
/// bytes beside it: a `tsi.origin` span is what names an id in a failure print.
struct Probe {
    facts: Vec<FactOut>,
    coverage: BTreeMap<String, bool>,
    source: Vec<u8>,
}

impl Probe {
    fn read(fixture: &str) -> Self {
        let stream = extract(&["--witness", "--family", "type", fixture]);
        let mut facts = Vec::new();
        let mut coverage = BTreeMap::new();
        for line in stream.lines() {
            let row: FlatFact = serde_json::from_str(line)
                .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"));
            match row {
                FlatFact::Fact(fact) => facts.push(fact),
                FlatFact::Coverage(claim) => {
                    coverage.insert(claim.relation, claim.complete);
                }
                _ => {}
            }
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture);
        let source = std::fs::read(&path).expect("the fixture is readable");
        Self {
            facts,
            coverage,
            source,
        }
    }

    fn rows(&self, wanted: &str) -> Vec<&FactOut> {
        self.facts
            .iter()
            .filter(|fact| fact.relation == wanted)
            .collect()
    }

    /// The source text a type id's single `tsi.origin` covers.
    fn origin_text(&self, id: u32) -> String {
        let mut found: Vec<String> = Vec::new();
        for fact in self.rows("tsi.origin") {
            if as_id(&fact.args[0]) != Some(id) {
                continue;
            }
            let (start, end) = as_span(&fact.args[2]).expect("an origin carries a span");
            let slice = &self.source[start as usize..end as usize];
            found.push(String::from_utf8_lossy(slice).to_string());
        }
        assert_eq!(found.len(), 1, "id {id} has {} origin rows", found.len());
        found.remove(0)
    }

    /// The one id `relation` names whose origin covers `written`.
    fn id_of(&self, wanted: &str, written: &str) -> u32 {
        let mut found: Vec<u32> = self
            .rows(wanted)
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[0]))
            .filter(|id| self.origin_text(*id) == written)
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{wanted} named {} ids written `{written}`",
            found.len()
        );
        found.remove(0)
    }

    /// The `tsi.edge` row an owner declares under `label`.
    fn edge(&self, owner: u32, label: &str) -> &FactOut {
        let mut found: Vec<&FactOut> = self
            .rows("tsi.edge")
            .into_iter()
            .filter(|fact| {
                as_id(&fact.args[1]) == Some(owner) && as_text(&fact.args[2]) == Some(label)
            })
            .collect();
        assert_eq!(
            found.len(),
            1,
            "owner {owner} has {} `{label}` edges",
            found.len()
        );
        found.remove(0)
    }

    /// The type parameter a callee declares at `position`.
    fn parameter(&self, callee: u32, position: i64) -> u32 {
        let mut found: Vec<u32> = self
            .rows("tsi.parameter")
            .into_iter()
            .filter(|fact| {
                as_id(&fact.args[1]) == Some(callee) && as_int(&fact.args[2]) == Some(position)
            })
            .inspect(|fact| assert_eq!(as_atom(&fact.args[3]), Some("unspecified")))
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "callee {callee} declares {} parameters at {position}",
            found.len()
        );
        found.remove(0)
    }

    fn carries(&self, wanted: &str, id: u32) -> bool {
        self.rows(wanted)
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
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
        Arg::Int(value) => Some(*value),
        _ => None,
    }
}

fn as_span(arg: &Arg) -> Option<(u32, u32)> {
    match arg {
        Arg::Span(_, start, end) => Some((*start, *end)),
        _ => None,
    }
}

/// The wire every consumer is already on: the rows are new under the flag and
/// nowhere else. `golden_parity` is the same claim over the whole corpus.
#[test]
fn flag_off_emits_no_fact_row() {
    for fixture in [TS_PROBE, RUST_PROBE, GO_PROBE] {
        let stream = extract(&["--family", "type", fixture]);
        assert!(!stream.is_empty(), "{fixture} produced no rows");
        for line in stream.lines() {
            let row: serde_json::Value = serde_json::from_str(line).expect("a row is JSON");
            assert_ne!(row["record"], "fact", "{fixture}: {line}");
            assert!(row.get("fact").is_none(), "{fixture}: {line}");
        }
    }
}

/// Criterion 6, the ts half: a declared field carries its name, its ordinal
/// and the two modifiers a parse can read off the token.
#[test]
fn ts_product_carries_named_positioned_fields() {
    let probe = Probe::read(TS_PROBE);
    let user = probe.id_of("tsi.product", "User");
    let element = probe.parameter(user, 0);
    assert_eq!(probe.origin_text(element), "T");

    let id_edge = probe.edge(user, "id");
    assert_eq!(as_id(&id_edge.args[3]), Some(element));
    assert_eq!(as_int(&id_edge.args[4]), Some(0));
    let id_edge_id = as_id(&id_edge.args[0]).expect("an edge declares its own id");
    assert!(probe.carries("ts.readonly", id_edge_id));
    assert!(!probe.carries("ts.optional", id_edge_id));

    let name_edge = probe.edge(user, "name");
    // `string` is a keyword primitive: a class, never an origin range.
    let string = as_id(&name_edge.args[3]).unwrap();
    assert!(probe.carries("tsi.primitive", string));
    assert!(!probe.carries("tsi.origin", string));
    assert_eq!(as_int(&name_edge.args[4]), Some(1));
    let name_edge_id = as_id(&name_edge.args[0]).expect("an edge declares its own id");
    assert!(probe.carries("ts.optional", name_edge_id));
    assert!(!probe.carries("ts.readonly", name_edge_id));
}

/// `ts.interface` is the ts-native row the kernel has no word for, and an
/// interface's type parameter is a declaration of its own.
#[test]
fn ts_interface_declares_its_parameter() {
    let probe = Probe::read(TS_PROBE);
    let mapper = probe.id_of("ts.interface", "Mapper");
    assert!(probe.carries("tsi.product", mapper));
    let element = probe.parameter(mapper, 0);
    assert_eq!(probe.origin_text(element), "T");
    let user = probe.id_of("tsi.product", "User");
    assert_ne!(
        element,
        probe.parameter(user, 0),
        "two declarations share one parameter id"
    );
}

/// The one conformance a parse can state: an explicit `implements` clause.
#[test]
fn ts_conforms_comes_from_the_implements_clause() {
    let probe = Probe::read(TS_PROBE);
    let user = probe.id_of("tsi.product", "User");
    let mapper = probe.id_of("ts.interface", "Mapper");
    let stated: Vec<&FactOut> = probe
        .rows("tsi.conforms")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(user))
        .collect();
    assert_eq!(stated.len(), 1);
    assert_eq!(as_id(&stated[0].args[1]), Some(mapper));
    assert_eq!(as_atom(&stated[0].args[2]), Some("syntax"));
}

/// A callable's inputs are its written parameter texts, in order, and its
/// output is the written return type.
#[test]
fn ts_callable_inputs_and_output() {
    let probe = Probe::read(TS_PROBE);
    let map = probe.id_of("tsi.callable", "map");
    let result = probe.parameter(map, 0);
    assert_eq!(probe.origin_text(result), "U");

    let inputs = probe.rows("tsi.input");
    let written: Vec<&FactOut> = inputs
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(map))
        .collect();
    assert_eq!(written.len(), 1);
    assert_eq!(as_int(&written[0].args[1]), Some(0));
    assert_eq!(
        probe.origin_text(as_id(&written[0].args[2]).unwrap()),
        "(element: T) => U"
    );

    let returned: Vec<&FactOut> = probe
        .rows("tsi.output")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(map))
        .collect();
    assert_eq!(returned.len(), 1);
    assert_eq!(as_int(&returned[0].args[1]), Some(0));
    assert_eq!(as_id(&returned[0].args[2]), Some(result));
}

/// A written `Name<Args>` alias body is the one type application the tier can
/// spell. It says nothing about the shape the application computes, and its
/// coverage row is what makes that silence readable.
#[test]
fn ts_written_generic_argument_is_a_call_not_a_shape() {
    let probe = Probe::read(TS_PROBE);
    let query = probe.id_of("tsi.called", "Query");
    let called = probe.rows("tsi.called");
    let row = called
        .iter()
        .find(|fact| as_id(&fact.args[0]) == Some(query))
        .expect("the alias is a call");
    assert_eq!(probe.origin_text(as_id(&row.args[1]).unwrap()), "Partial");

    let list = as_id(&row.args[2]).expect("a call names its argument list");
    let arguments: Vec<&FactOut> = probe
        .rows("tsi.argument")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(list))
        .collect();
    assert_eq!(arguments.len(), 1);
    assert_eq!(as_int(&arguments[0].args[1]), Some(0));
    // The origin of a written application spans its head (rust D1 parity);
    // the argument is itself a call on `User`.
    let argument = as_id(&arguments[0].args[2]).unwrap();
    assert_eq!(probe.origin_text(argument), "User");
    assert!(
        called.iter().any(|fact| as_id(&fact.args[0]) == Some(argument)),
        "`User<number>` states its own call"
    );

    let owned = probe
        .rows("tsi.edge")
        .into_iter()
        .filter(|fact| as_id(&fact.args[1]) == Some(query))
        .count();
    assert_eq!(owned, 0, "the tier claimed a shape for a computed type");
    assert_eq!(probe.coverage.get("tsi.edge"), Some(&false));
}

/// Criterion 6, the rust half.
#[test]
fn rust_struct_carries_fields_and_parameter() {
    let probe = Probe::read(RUST_PROBE);
    let user = probe.id_of("tsi.product", "User");
    let element = probe.parameter(user, 0);
    assert_eq!(probe.origin_text(element), "T");

    let id_edge = probe.edge(user, "id");
    assert_eq!(as_id(&id_edge.args[3]), Some(element));
    assert_eq!(as_int(&id_edge.args[4]), Some(0));

    let name_edge = probe.edge(user, "name");
    assert_eq!(
        probe.origin_text(as_id(&name_edge.args[3]).unwrap()),
        "Option"
    );
    assert_eq!(as_int(&name_edge.args[4]), Some(1));
}

/// Criterion 7's syntax half: the impl block is a row, the conformance it
/// states is a row, and the associated type inside it is not.
#[test]
fn rust_trait_and_impl_without_the_associated_type() {
    let probe = Probe::read(RUST_PROBE);
    let mapper = probe.id_of("rust.trait", "Mapper");
    let user = probe.id_of("tsi.product", "User");

    let blocks = probe.rows("rust.impl");
    assert_eq!(blocks.len(), 1);
    assert_eq!(as_id(&blocks[0].args[1]), Some(user));
    assert_eq!(as_id(&blocks[0].args[2]), Some(mapper));

    let stated: Vec<&FactOut> = probe
        .rows("tsi.conforms")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(user))
        .collect();
    assert_eq!(stated.len(), 1);
    assert_eq!(as_id(&stated[0].args[1]), Some(mapper));
    assert_eq!(as_atom(&stated[0].args[2]), Some("syntax"));

    assert!(probe.rows("rust.assoc").is_empty());
    assert!(!probe.coverage.contains_key("rust.assoc"));
}

/// Identity rule 1 on the wire: one id per written text per file, one origin
/// per id, and a class in place of an origin where the language declares it.
#[test]
fn one_id_per_written_name() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let probe = Probe::read(fixture);
        let declared: BTreeSet<u32> = probe
            .rows("tsi.type")
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect();
        let classed: BTreeSet<u32> = probe
            .rows("tsi.primitive")
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect();
        let mut origins: BTreeMap<u32, usize> = BTreeMap::new();
        for fact in probe.rows("tsi.origin") {
            let id = as_id(&fact.args[0]).expect("an origin names a type");
            *origins.entry(id).or_default() += 1;
        }
        for id in &declared {
            // No range in this file declares a primitive, so it carries its
            // class instead of an origin.
            let wanted = if classed.contains(id) { None } else { Some(&1) };
            assert_eq!(origins.get(id), wanted, "{fixture}: id {id}");
        }
    }
    let probe = Probe::read(TS_PROBE);
    let user = probe.id_of("tsi.product", "User");
    assert_eq!(
        as_id(&probe.edge(user, "name").args[3]),
        as_id(&probe.edge(user, "label").args[3]),
        "two fields written `string` took two ids"
    );
}

/// The registry is the contract, and the reverse door is the consumer that
/// enforces it. A row the door rejects is a producer defect.
#[test]
fn every_row_is_in_the_registry_and_ingests() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let probe = Probe::read(fixture);
        assert!(!probe.facts.is_empty(), "{fixture} produced no fact rows");
        for fact in &probe.facts {
            let row = relation(&fact.relation)
                .unwrap_or_else(|| panic!("{fixture}: {} is not in the registry", fact.relation));
            assert_eq!(
                fact.args.len(),
                row.args.len(),
                "{fixture}: {} arity",
                fact.relation
            );
        }
        let stream = extract(&["--witness", "--family", "type", fixture]);
        let mut door = Command::new(env!("CARGO_BIN_EXE_extract"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--ingest", "/dev/stdin"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("extract binary runs");
        door.stdin
            .take()
            .expect("stdin is piped")
            .write_all(stream.as_bytes())
            .expect("the stream is written");
        let landed = door.wait_with_output().expect("the door exits");
        assert!(
            landed.status.success(),
            "{fixture} stderr: {}",
            String::from_utf8_lossy(&landed.stderr)
        );
    }
}

/// The door's id closure, run on the producer's own side: an id no row
/// declares would leave a v7 import chasing a node that is not there.
#[test]
fn no_argument_names_an_undeclared_id() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let probe = Probe::read(fixture);
        let mut declared: BTreeSet<u32> = BTreeSet::new();
        for fact in &probe.facts {
            let position = match fact.relation.as_str() {
                "tsi.type" | "tsi.edge" => 0,
                "tsi.called" => 2,
                _ => continue,
            };
            if let Some(id) = as_id(&fact.args[position]) {
                declared.insert(id);
            }
        }
        for fact in &probe.facts {
            for arg in &fact.args {
                if let Some(id) = as_id(arg) {
                    assert!(
                        declared.contains(&id),
                        "{fixture}: {} names undeclared id {id}",
                        fact.relation
                    );
                }
            }
        }
    }
}

/// Coverage names what the run touched and nothing else: a relation the pass
/// never emitted must not claim a partial reading of it.
#[test]
fn coverage_names_every_emitted_relation_and_no_other() {
    for fixture in [TS_PROBE, RUST_PROBE] {
        let probe = Probe::read(fixture);
        let emitted: BTreeSet<&str> = probe
            .facts
            .iter()
            .map(|fact| fact.relation.as_str())
            .collect();
        for name in &emitted {
            assert_eq!(
                probe.coverage.get(*name),
                Some(&false),
                "{fixture}: {name} has no partial coverage row"
            );
        }
        for (name, complete) in &probe.coverage {
            assert!(!complete, "{fixture}: a parse claimed complete {name}");
            if name.starts_with("extract.") {
                continue;
            }
            assert!(
                emitted.contains(name.as_str()),
                "{fixture}: coverage for unemitted {name}"
            );
        }
    }
}
