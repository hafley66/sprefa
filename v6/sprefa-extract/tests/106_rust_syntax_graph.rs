//! The rust syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.rs` under `--witness --family type`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `46c39def5` the
//! fixture is absent; with it restored and `rust_type_edges.rs` at that sha,
//! all 7 cases fail. Per defect, the row the base sha lacks:
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

/// D2: a written `Name<Args>` states its application wherever it stands, a
/// nested one states its own, and a tuple states its ordered members.
#[test]
fn every_written_application_states_its_call() {
    let probe = Probe::read();
    let trail = probe.id_of("tsi.product", "Trail");

    let steps = as_id(&probe.edge(trail, "steps").args[3]).expect("the field names a type");
    let (outer, outer_list) = probe.call(steps);
    assert_eq!(probe.origin_text(outer), "Vec");
    let carried = probe.arguments(outer_list);
    assert_eq!(carried.len(), 1);
    assert_eq!(carried[0].0, 0);
    let (inner, inner_list) = probe.call(carried[0].1);
    assert_eq!(probe.origin_text(inner), "Option");
    let element = probe.arguments(inner_list);
    assert_eq!(element.len(), 1);
    assert!(
        probe.carries("tsi.parameter", element[0].1),
        "the innermost argument is not the declared parameter"
    );

    let outcome = as_id(&probe.edge(trail, "outcome").args[3]).expect("the field names a type");
    let (returned, returned_list) = probe.call(outcome);
    assert_eq!(probe.origin_text(returned), "Result");
    let parts = probe.arguments(returned_list);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts[0].0, 0);
    assert_eq!(probe.origin_text(parts[1].1), "Error");

    let label = as_id(&probe.edge(trail, "label").args[3]).expect("the field names a type");
    assert!(probe.carries("tsi.product", label), "a tuple states no shape");
    let members = probe.edges_of(label);
    let labels: Vec<&str> = members.keys().map(String::as_str).collect();
    assert_eq!(labels, vec!["0", "1"]);
    assert_eq!(members["0"].1, 0);
    assert_eq!(members["1"].1, 1);
    assert_eq!(probe.origin_text(members["0"].0), "String");

    let callees: BTreeSet<String> = probe
        .rows("tsi.called")
        .into_iter()
        .map(|fact| probe.origin_text(as_id(&fact.args[1]).expect("a call names a callee")))
        .collect();
    let wanted: BTreeSet<String> = ["Box", "Option", "Result", "Vec"]
        .into_iter()
        .map(str::to_string)
        .collect();
    assert_eq!(callees, wanted);
    assert_eq!(probe.rows("tsi.called").len(), 4, "one call per written text");
}

/// D3: a tuple variant labels its payload by ordinal, a struct variant by
/// field name, and a unit variant states no shape at all.
#[test]
fn a_variant_carries_its_payload() {
    let probe = Probe::read();
    let step = probe.id_of("tsi.sum", "Step");
    let arms = probe.edges_of(step);
    let labels: Vec<&str> = arms.keys().map(String::as_str).collect();
    assert_eq!(labels, vec!["Failed", "Idle", "Retry"]);
    assert_eq!(arms["Idle"].1, 0);
    assert_eq!(arms["Retry"].1, 1);
    assert_eq!(arms["Failed"].1, 2);

    let idle = arms["Idle"].0;
    assert!(!probe.carries("tsi.product", idle), "a unit variant claimed a shape");
    assert!(probe.edges_of(idle).is_empty());

    let retry = arms["Retry"].0;
    assert!(probe.carries("tsi.product", retry));
    let payload = probe.edges_of(retry);
    let carried: Vec<&str> = payload.keys().map(String::as_str).collect();
    assert_eq!(carried, vec!["0", "1"]);
    assert_eq!(payload["0"].1, 0);
    assert_eq!(payload["1"].1, 1);

    let failed = arms["Failed"].0;
    assert!(probe.carries("tsi.product", failed));
    let named = probe.edges_of(failed);
    let fields: Vec<&str> = named.keys().map(String::as_str).collect();
    assert_eq!(fields, vec!["code", "reason"]);
    assert_eq!(named["reason"].1, 0);
    assert_eq!(named["code"].1, 1);
    assert_eq!(probe.origin_text(named["reason"].0), "String");
}

/// D4: a method is reachable from the type that owns it, and from the trait
/// that declares it, positioned by its index among the block's fns.
#[test]
fn an_owner_reaches_its_methods() {
    let probe = Probe::read();
    let trail = probe.id_of("tsi.product", "Trail");
    let render_trait = probe.id_of("rust.trait", "Render");

    let owned = probe.edges_of(trail);
    for label in ["is_empty", "clear", "render"] {
        assert!(
            probe.carries("tsi.callable", owned[label].0),
            "the `{label}` edge does not name a callable"
        );
    }
    assert_eq!(owned["is_empty"].1, 0);
    assert_eq!(owned["clear"].1, 1);
    assert_eq!(owned["render"].1, 0, "a trait impl block counts on its own");

    let declared = probe.edges_of(render_trait);
    let contract: Vec<&str> = declared.keys().map(String::as_str).collect();
    assert_eq!(contract, vec!["render"]);
    assert_eq!(declared["render"].1, 0);
    assert_ne!(
        declared["render"].0, owned["render"].0,
        "the declaration and the impl took one callable"
    );

    let reached: BTreeSet<u32> = probe
        .rows("tsi.edge")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[3]))
        .collect();
    let callables: BTreeSet<u32> = probe
        .rows("tsi.callable")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[0]))
        .collect();
    assert_eq!(callables.len(), 4, "the fixture declares four callables");
    assert!(
        callables.iter().all(|id| reached.contains(id)),
        "a callable no owner reaches"
    );
}

/// D5: a const and a static state the type written at their ident, and state
/// nothing else: no id of their own, no edge, no callable.
#[test]
fn a_const_and_a_static_state_their_type() {
    let probe = Probe::read();
    let stated: BTreeMap<String, u32> = probe
        .rows("tsi.has_type")
        .into_iter()
        .map(|fact| {
            let (start, end) = as_span(&fact.args[0]).expect("an occurrence is a range");
            (
                probe.slice(start, end),
                as_id(&fact.args[1]).expect("an occurrence names a type"),
            )
        })
        .collect();
    let occurrences: Vec<&str> = stated.keys().map(String::as_str).collect();
    assert_eq!(occurrences, vec!["BANNER", "RETRY_LIMIT"]);

    let limit = probe.offset_of("RETRY_LIMIT");
    let banner = probe.offset_of("BANNER");
    let ranges: BTreeSet<(u32, u32)> = probe
        .rows("tsi.has_type")
        .into_iter()
        .filter_map(|fact| as_span(&fact.args[0]))
        .collect();
    assert_eq!(
        ranges,
        [(limit, limit + 11), (banner, banner + 6)]
            .into_iter()
            .collect()
    );
    assert_eq!(probe.origin_text(stated["BANNER"]), "str");
}

/// D6: a name the language declares carries a class and no origin, one id per
/// class per file, and `String` is a library type rather than a class.
#[test]
fn a_builtin_carries_a_class_and_no_origin() {
    let probe = Probe::read();
    let mut classes: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for fact in probe.rows("tsi.primitive") {
        let id = as_id(&fact.args[0]).expect("a class names a type");
        let atom = as_atom(&fact.args[1]).expect("a class is an atom");
        assert!(CLASSES.contains(&atom), "`{atom}` is not a declared class");
        assert!(!probe.has_origin(id), "class `{atom}` claimed an origin");
        assert!(probe.carries("tsi.type", id), "class `{atom}` declares no id");
        classes.entry(atom.to_string()).or_default().push(id);
    }
    let named: Vec<&str> = classes.keys().map(String::as_str).collect();
    assert_eq!(named, vec!["bool", "str", "u32", "u64", "unit"]);
    for (atom, ids) in &classes {
        assert_eq!(ids.len(), 1, "class `{atom}` took {} ids", ids.len());
    }

    let trail = probe.id_of("tsi.product", "Trail");
    let label = as_id(&probe.edge(trail, "label").args[3]).expect("the field names a type");
    let members = probe.edges_of(label);
    assert_eq!(probe.class(members["1"].0), Some("u32".to_string()));
    assert_eq!(probe.class(members["0"].0), None, "`String` is not a class");
    assert_eq!(probe.origin_text(members["0"].0), "String");

    let clear = probe.edges_of(trail)["clear"].0;
    let returned: Vec<u32> = probe
        .rows("tsi.output")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(clear))
        .filter_map(|fact| as_id(&fact.args[2]))
        .collect();
    assert_eq!(returned.len(), 1);
    assert_eq!(probe.class(returned[0]), Some("unit".to_string()));
}

/// D7: `&self` takes no input slot, so the first written parameter stands at
/// position 0, and the mode it is written in stays the checker's row.
#[test]
fn a_receiver_takes_no_input_slot() {
    let probe = Probe::read();
    let owned = probe.edges_of(probe.id_of("tsi.product", "Trail"));
    let inputs = |callable: u32| -> Vec<(i64, u32)> {
        let mut found: Vec<(i64, u32)> = probe
            .rows("tsi.input")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(callable))
            .map(|fact| {
                (
                    as_int(&fact.args[1]).expect("an input carries a position"),
                    as_id(&fact.args[2]).expect("an input names a type"),
                )
            })
            .collect();
        found.sort();
        found
    };

    assert!(inputs(owned["is_empty"].0).is_empty());
    assert!(inputs(owned["clear"].0).is_empty());

    let written = inputs(owned["render"].0);
    assert_eq!(written.len(), 1);
    assert_eq!(written[0].0, 0, "the receiver shifted the written input");
    assert_eq!(probe.origin_text(written[0].1), "Formatter");

    for unstated in ["rust.ownership", "rust.lifetime", "rust.assoc"] {
        assert!(probe.rows(unstated).is_empty(), "a parse claimed {unstated}");
    }
}
