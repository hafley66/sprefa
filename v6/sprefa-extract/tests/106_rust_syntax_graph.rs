//! The rust syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.rs` under `--witness --family type`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `46c39def5` the
//! fixture is absent, so every case below reads an empty fact set. Per defect,
//! the row the base sha lacks:
//!
//! - D1: `std::fmt::Result` takes its origin span from `segments.first()`, so
//!   the origin slice reads `std`, never `Result`.
//! - D2: `tsi.called` fires only from an `Item::Type` alias body; a field, an
//!   input, an output and a variant payload written `Name<Args>` emit none.
//! - D3: an enum variant is a bare named id with no `tsi.product` and no edge
//!   to its payload.
//! - D4: `tsi.callable` mints an id no owner reaches; the struct has no edge
//!   to `is_empty`, `clear` or `render`.
//! - D5: `Item::Const` and `Item::Static` fall to `_ => {}`, so the run emits
//!   zero `tsi.has_type`.
//! - D6: `u32`, `u64`, `bool` and `str` mint `tsi.type` plus `tsi.origin` at
//!   their reference span, and the run emits zero `tsi.primitive`.
//! - D7: `&self` is skipped, which is the shape both sides agree on.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/tsi/probe_graph.rs";

/// The 17 names the v7 prelude declares for rust plus the empty tuple's class.
const CLASSES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "u8", "u16", "u32", "u64", "u128", "f32", "f64", "bool",
    "char", "str", "usize", "isize", "unit",
];

/// One witnessed run over the fixture with the fixture's own bytes beside it:
/// a `tsi.origin` span is what names an id in a failure print.
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

    /// The byte offset of a written form, which must occur once in the fixture.
    fn offset_of(&self, written: &str) -> u32 {
        let text = String::from_utf8_lossy(&self.source).to_string();
        let found: Vec<usize> = text.match_indices(written).map(|(at, _)| at).collect();
        assert_eq!(found.len(), 1, "`{written}` occurs {} times", found.len());
        found[0] as u32
    }

    /// The single `tsi.origin` range an id carries.
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

    /// The one id `relation` names whose origin covers `written`.
    fn id_of(&self, wanted: &str, written: &str) -> u32 {
        let mut found: Vec<u32> = self
            .rows(wanted)
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[0]))
            .filter(|id| self.has_origin(*id) && self.origin_text(*id) == written)
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{wanted} named {} ids written `{written}`",
            found.len()
        );
        found.remove(0)
    }

    fn has_origin(&self, id: u32) -> bool {
        self.rows("tsi.origin")
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
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

    /// Every edge an owner declares, as label to (target, position).
    fn edges_of(&self, owner: u32) -> BTreeMap<String, (u32, i64)> {
        let mut out = BTreeMap::new();
        for fact in self.rows("tsi.edge") {
            if as_id(&fact.args[1]) != Some(owner) {
                continue;
            }
            let label = as_text(&fact.args[2]).expect("an edge carries a label");
            let target = as_id(&fact.args[3]).expect("an edge carries a target");
            let position = as_int(&fact.args[4]).expect("an edge carries a position");
            out.insert(label.to_string(), (target, position));
        }
        out
    }

    fn carries(&self, wanted: &str, id: u32) -> bool {
        self.rows(wanted)
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
    }

    /// The `tsi.called` row an application id states, as (callee, list).
    fn call(&self, result: u32) -> (u32, u32) {
        let mut found: Vec<(u32, u32)> = self
            .rows("tsi.called")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(result))
            .map(|fact| {
                (
                    as_id(&fact.args[1]).expect("a call names a callee"),
                    as_id(&fact.args[2]).expect("a call names a list"),
                )
            })
            .collect();
        assert_eq!(found.len(), 1, "id {result} has {} calls", found.len());
        found.remove(0)
    }

    /// An argument list's members, position to type id.
    fn arguments(&self, list: u32) -> Vec<(i64, u32)> {
        let mut out: Vec<(i64, u32)> = self
            .rows("tsi.argument")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(list))
            .map(|fact| {
                (
                    as_int(&fact.args[1]).expect("an argument carries a position"),
                    as_id(&fact.args[2]).expect("an argument names a type"),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// The class atom an id is bound to, if the run called it primitive.
    fn class(&self, id: u32) -> Option<String> {
        self.rows("tsi.primitive")
            .into_iter()
            .find(|fact| as_id(&fact.args[0]) == Some(id))
            .and_then(|fact| as_atom(&fact.args[1]).map(str::to_string))
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

/// D1: a qualified path is named by its last segment, and the full written
/// text still keys its own id, so `std::fmt::Result` and `Result` stay apart.
#[test]
fn qualified_path_spans_its_last_segment() {
    let probe = Probe::read();
    let trail = probe.id_of("tsi.product", "Trail");
    let target = as_id(&probe.edge(trail, "rendered").args[3]).expect("the field names a type");
    let written = probe.offset_of("std::fmt::Result");
    assert_eq!(probe.origin(target), (written + 10, written + 16));
    assert_eq!(probe.origin_text(target), "Result");

    for fact in probe.rows("tsi.origin") {
        let (start, end) = as_span(&fact.args[2]).expect("an origin carries a span");
        assert_ne!(probe.slice(start, end), "std", "an origin spans a qualifier");
    }

    let applied = as_id(&probe.edge(trail, "outcome").args[3]).expect("the field names a type");
    assert_ne!(applied, target, "two written texts took one id");
}
