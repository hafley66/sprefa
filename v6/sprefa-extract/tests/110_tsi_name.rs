//! `tsi.name(Id, Text)`: the spelling a consumer prints for a type id, so a
//! renderer never has to open the file behind a `tsi.origin` span.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `93f9f8ecf`
//! the registry has no `tsi.name` row and no tier writes one, so every case
//! below reads an empty `tsi.name` set and fails on its first assertion.

use std::collections::BTreeMap;
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const RUST_PROBE: &str = "tests/fixtures/tsi/probe_graph.rs";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";

struct Probe {
    facts: Vec<FactOut>,
    source: Vec<u8>,
}

impl Probe {
    fn read(fixture: &str) -> Self {
        let output = Command::new(env!("CARGO_BIN_EXE_extract"))
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["--witness", "--family", "type", fixture])
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
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture);
        let source = std::fs::read(&path).expect("the fixture is readable");
        Self { facts, source }
    }

    fn rows(&self, wanted: &str) -> Vec<&FactOut> {
        self.facts
            .iter()
            .filter(|fact| fact.relation == wanted)
            .collect()
    }

    /// id -> spelling, asserting one `tsi.name` row per id.
    fn names(&self) -> BTreeMap<u32, String> {
        let mut names = BTreeMap::new();
        for fact in self.rows("tsi.name") {
            let id = as_id(&fact.args[0]).expect("tsi.name arg 0 is an id");
            let text = as_text(&fact.args[1]).expect("tsi.name arg 1 is text");
            let prior = names.insert(id, text.to_string());
            assert!(prior.is_none(), "id {id} carries two tsi.name rows");
        }
        names
    }

    /// id -> the source text its `tsi.origin` span covers.
    fn origin_texts(&self) -> BTreeMap<u32, String> {
        let mut texts = BTreeMap::new();
        for fact in self.rows("tsi.origin") {
            let id = as_id(&fact.args[0]).expect("tsi.origin arg 0 is an id");
            if let Some((start, end)) = as_span(&fact.args[2]) {
                let slice = &self.source[start as usize..end as usize];
                texts.insert(id, String::from_utf8_lossy(slice).to_string());
            }
        }
        texts
    }

    fn classes(&self) -> BTreeMap<u32, String> {
        self.rows("tsi.primitive")
            .into_iter()
            .filter_map(|fact| Some((as_id(&fact.args[0])?, as_atom(&fact.args[1])?.to_string())))
            .collect()
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

fn as_span(arg: &Arg) -> Option<(u32, u32)> {
    match arg {
        Arg::Span(_, start, end) => Some((*start, *end)),
        _ => None,
    }
}

/// Every named id carries a `tsi.name` that is the written text keyed on
/// (`Vec<Option<T>>`, `std::fmt::Result`), of which the origin span is the
/// last segment; every primitive carries its class as its name. Tuples and
/// impl blocks have no name row.
fn every_named_id_spells_its_origin(fixture: &str) {
    let probe = Probe::read(fixture);
    let names = probe.names();
    let origins = probe.origin_texts();
    let classes = probe.classes();
    assert!(!names.is_empty(), "{fixture}: no tsi.name row");
    for fact in probe.rows("tsi.type") {
        let id = as_id(&fact.args[0]).expect("tsi.type arg 0 is an id");
        match (names.get(&id), origins.get(&id), classes.get(&id)) {
            (Some(name), _, Some(class)) => {
                let written = if class == "unit" {
                    "()"
                } else {
                    class.as_str()
                };
                assert_eq!(name, written, "{fixture}: primitive {id} names its class");
            }
            (Some(name), Some(origin), None) => {
                assert!(
                    name.ends_with(origin.as_str()) || name.contains(&format!("{origin}<")),
                    "{fixture}: id {id} name `{name}` does not carry origin text `{origin}`"
                );
            }
            (None, Some(origin), None) => {
                assert!(
                    origin.starts_with('(') || origin == "impl",
                    "{fixture}: id {id} written `{origin}` has no tsi.name"
                );
            }
            (None, _, Some(_)) => panic!("{fixture}: primitive {id} has no tsi.name"),
            (Some(_), None, None) => panic!("{fixture}: id {id} named with no origin or class"),
            (None, None, None) => panic!("{fixture}: id {id} has no origin, class or name"),
        }
    }
}

#[test]
fn rust_named_ids_spell_their_origin() {
    every_named_id_spells_its_origin(RUST_PROBE);
}

#[test]
fn ts_named_ids_spell_their_origin() {
    every_named_id_spells_its_origin(TS_PROBE);
}

#[test]
fn rust_names_are_the_written_words() {
    let probe = Probe::read(RUST_PROBE);
    let names = probe.names();
    let mut spelled: Vec<&str> = names.values().map(String::as_str).collect();
    spelled.sort_unstable();
    for wanted in [
        "Step",
        "Trail",
        "Error",
        "Result<u64, Error>",
        "std::fmt::Result",
        "Vec<Option<T>>",
        "&mut fmt::Formatter",
        "Step::Idle",
        "u32",
        "bool",
        "str",
        "()",
        "T",
        "render",
        "is_empty",
    ] {
        assert!(
            spelled.contains(&wanted),
            "{wanted} absent from {spelled:?}"
        );
    }
}
