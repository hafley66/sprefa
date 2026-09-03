//! The go syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.go` under `--witness --family type`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `1696ce96a`
//! `extract --witness --family type` emits zero tsi rows for any `.go` input
//! (`grep -c '"tsi\.' src/lang/go.rs` is 0), so every case below reads an
//! empty fact set and fails on its first assertion.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/tsi/probe_graph.go";

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

    fn has_origin(&self, id: u32) -> bool {
        self.rows("tsi.origin")
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
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

    /// The one id whose `tsi.name` spells `written`.
    fn named(&self, written: &str) -> u32 {
        let found: Vec<u32> = self
            .names()
            .into_iter()
            .filter(|(_, name)| name == written)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(found.len(), 1, "{} ids are named `{written}`", found.len());
        found[0]
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
                texts.insert(id, self.slice(start, end));
            }
        }
        texts
    }

    /// id -> the class atom, over every `tsi.primitive` row.
    fn classes(&self) -> BTreeMap<u32, String> {
        self.rows("tsi.primitive")
            .into_iter()
            .filter_map(|fact| Some((as_id(&fact.args[0])?, as_atom(&fact.args[1])?.to_string())))
            .collect()
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

    /// The second ids of every `relation` row whose first id is `owner`.
    fn pairs_from(&self, wanted: &str, owner: u32) -> BTreeSet<u32> {
        self.rows(wanted)
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(owner))
            .filter_map(|fact| as_id(&fact.args[1]))
            .collect()
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

    /// A callable's `relation` slots (`tsi.input` or `tsi.output`), position to type.
    fn slots(&self, wanted: &str, callable: u32) -> Vec<(i64, u32)> {
        let mut out: Vec<(i64, u32)> = self
            .rows(wanted)
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(callable))
            .map(|fact| {
                (
                    as_int(&fact.args[1]).expect("a slot carries a position"),
                    as_id(&fact.args[2]).expect("a slot names a type"),
                )
            })
            .collect();
        out.sort();
        out
    }

    /// The parameter an owner declares at `position`.
    fn parameter(&self, owner: u32, position: i64) -> u32 {
        let mut found: Vec<u32> = self
            .rows("tsi.parameter")
            .into_iter()
            .filter(|fact| {
                as_id(&fact.args[1]) == Some(owner) && as_int(&fact.args[2]) == Some(position)
            })
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "owner {owner} declares {} parameters at {position}",
            found.len()
        );
        found.remove(0)
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

/// `type S struct { A T; B *U; V }`: one edge per named field in written
/// order, the pointer stripped, the embedded field labelled by its own name.
#[test]
fn struct_declares_named_fields_and_an_embedding() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let base = probe.id_of("tsi.product", "Base");
    let edges = probe.edges_of(node);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["Base", "Index", "Len", "Next", "Render", "Tags", "Value"]);

    let element = probe.parameter(node, 0);
    assert_eq!(edges["Value"], (element, 0));
    let (next, _) = edges["Next"];
    assert_eq!(probe.names()[&next], "Node[T, K]");
    assert_eq!(probe.origin_text(next), "Node");
    assert_eq!(edges["Next"].1, 1);
    let (tags, _) = edges["Tags"];
    assert_eq!(probe.origin_text(tags), "[]string");
    assert_eq!(edges["Tags"].1, 2);
    let (index, _) = edges["Index"];
    assert_eq!(probe.origin_text(index), "map[K]int64");
    assert_eq!(edges["Index"].1, 3);
    assert_eq!(edges["Base"], (base, 4));
    assert_eq!(probe.pairs_from("go.embedding", node), BTreeSet::from([base]));

    let id_field = probe.edge(base, "ID");
    assert_eq!(
        probe.class(as_id(&id_field.args[3]).unwrap()).as_deref(),
        Some("int64")
    );
}

/// A declared type origins at its declaring name, even when a field written
/// earlier in the file references it first.
#[test]
fn declared_type_origins_at_its_own_name() {
    let probe = Probe::read();
    let base = probe.id_of("tsi.product", "Base");
    let declared = probe.offset_of("type Base") + 5;
    assert_eq!(probe.origin(base), (declared, declared + 4));
    let base_rows = probe.rows("tsi.type").len();
    let named_base: Vec<u32> = probe
        .names()
        .into_iter()
        .filter(|(_, name)| name == "Base")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(named_base, [base], "one id is written `Base` among {base_rows}");
}

/// `type I interface { M(x T) U; Other }`: a product and `go.interface`, the
/// method reached by name, the embedded interface an embedding.
#[test]
fn interface_reaches_its_method_and_embeds_another() {
    let probe = Probe::read();
    let shape = probe.id_of("go.interface", "Shape");
    assert!(probe.carries("tsi.product", shape));
    let edges = probe.edges_of(shape);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["Area"]);
    let (area, position) = edges["Area"];
    assert_eq!(position, 0);
    assert!(probe.carries("tsi.callable", area));
    assert_eq!(probe.origin_text(area), "Area");
    let float64 = probe.named("float64");
    assert_eq!(probe.slots("tsi.input", area), [(0, float64)]);
    assert_eq!(probe.slots("tsi.output", area), [(0, float64)]);

    let stringer = probe.named("fmt.Stringer");
    assert_eq!(probe.origin_text(stringer), "Stringer");
    assert_eq!(
        probe.pairs_from("go.embedding", shape),
        BTreeSet::from([stringer])
    );
    assert!(probe.pairs_from("go.type_set", shape).is_empty());
}

/// `type C interface { ~int | string }`: one `go.type_set` per term with the
/// `~` dropped, and no edge or embedding.
#[test]
fn type_set_lists_its_terms_without_the_tilde() {
    let probe = Probe::read();
    let number = probe.id_of("go.interface", "Number");
    let int = probe.named("int");
    let string = probe.named("string");
    assert_eq!(
        probe.pairs_from("go.type_set", number),
        BTreeSet::from([int, string])
    );
    assert!(probe.edges_of(number).is_empty());
    assert!(probe.pairs_from("go.embedding", number).is_empty());
    assert_eq!(probe.rows("go.type_set").len(), 2);
}

/// `[T any, K comparable]` and `func F[T Number]`: one parameter per
/// position, a `bound` edge per constraint that is not `any`.
#[test]
fn type_parameters_carry_their_bounds() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let element = probe.parameter(node, 0);
    let key = probe.parameter(node, 1);
    assert_eq!(probe.names()[&element], "T");
    assert_eq!(probe.names()[&key], "K");
    assert!(probe.edges_of(element).is_empty());
    let comparable = probe.named("comparable");
    assert_eq!(probe.edges_of(key)["bound"], (comparable, 0));
    assert_eq!(probe.origin_text(comparable), "comparable");
    assert!(probe.class(comparable).is_none());

    let sum = probe.id_of("tsi.callable", "Sum");
    let bounded = probe.parameter(sum, 0);
    let number = probe.id_of("go.interface", "Number");
    assert_eq!(probe.edges_of(bounded)["bound"], (number, 0));
    assert_eq!(probe.slots("tsi.output", sum), [(0, bounded)]);
    assert!(!probe.names().values().any(|name| name == "any"));
}

/// `L[int]` written anywhere: one `tsi.called` per distinct written text
/// with the callee being the head and one argument per position.
#[test]
fn written_application_is_called_once() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let element = probe.parameter(node, 0);
    let key = probe.parameter(node, 1);
    assert_eq!(probe.rows("tsi.called").len(), 2);

    let generic = probe.named("Node[T, K]");
    let (callee, list) = probe.call(generic);
    assert_eq!(callee, node);
    assert_eq!(probe.arguments(list), [(0, element), (1, key)]);

    let concrete = probe.named("Node[int64, string]");
    assert_eq!(probe.origin_text(concrete), "Node");
    let (callee, list) = probe.call(concrete);
    assert_eq!(callee, node);
    let int64 = probe.named("int64");
    let string = probe.named("string");
    assert_eq!(probe.arguments(list), [(0, int64), (1, string)]);
}

/// `func (r *S) M(a A) (B, error)`: the receiver's type reaches the callable
/// by position among its methods, the receiver takes no input slot.
#[test]
fn method_is_reached_from_its_receiver_type() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let edges = probe.edges_of(node);
    let (render, render_at) = edges["Render"];
    let (len, len_at) = edges["Len"];
    assert_eq!((render_at, len_at), (0, 1));
    assert!(probe.carries("tsi.callable", render));
    assert!(probe.carries("tsi.callable", len));

    let int = probe.named("int");
    let bool = probe.named("bool");
    let string = probe.named("string");
    let error = probe.named("error");
    assert_eq!(probe.origin_text(error), "error");
    assert!(probe.class(error).is_none());
    assert_eq!(probe.slots("tsi.input", render), [(0, int), (1, bool)]);
    assert_eq!(probe.slots("tsi.output", render), [(0, string), (1, error)]);
    assert_eq!(probe.slots("tsi.input", len), []);
    assert_eq!(probe.slots("tsi.output", len), [(0, int)]);

    let receiver_element = probe.parameter(render, 0);
    assert_eq!(probe.names()[&receiver_element], "T");
    assert_ne!(receiver_element, probe.parameter(node, 0));
}

/// `func F(...)`: a named callable no owner reaches.
#[test]
fn free_function_has_no_owner() {
    let probe = Probe::read();
    let encode = probe.id_of("tsi.callable", "Encode");
    let sum = probe.id_of("tsi.callable", "Sum");
    for fact in probe.rows("tsi.edge") {
        let target = as_id(&fact.args[3]).expect("an edge carries a target");
        assert_ne!(target, encode, "an owner reaches Encode");
        assert_ne!(target, sum, "an owner reaches Sum");
    }
    let bytes = probe.named("[]byte");
    let bool = probe.named("bool");
    let byte = probe.named("byte");
    assert_eq!(probe.slots("tsi.input", encode), [(0, bytes), (1, bool)]);
    assert_eq!(probe.slots("tsi.output", encode), [(0, byte)]);
    assert_eq!(probe.rows("tsi.callable").len(), 5);
}

/// `type A = B` is a symbol that denotes; `type A B` is a type with an
/// `underlying` edge.
#[test]
fn alias_denotes_and_defined_type_has_an_underlying() {
    let probe = Probe::read();
    let symbols: Vec<u32> = probe
        .rows("tsi.symbol")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[0]))
        .collect();
    assert_eq!(symbols.len(), 1);
    let label = symbols[0];
    assert_eq!(probe.names()[&label], "Label");
    assert!(!probe.carries("tsi.type", label));
    let string = probe.named("string");
    assert_eq!(probe.pairs_from("tsi.denotes", label), BTreeSet::from([string]));

    let meters = probe.id_of("tsi.type", "Meters");
    let int64 = probe.named("int64");
    let edges = probe.edges_of(meters);
    assert_eq!(edges.len(), 1);
    assert_eq!(edges["underlying"], (int64, 0));
    assert!(!probe.carries("tsi.product", meters));
}

/// `const X T = ...` and `var Y U` carry the written type at the identifier;
/// an untyped const carries nothing.
#[test]
fn typed_values_carry_has_type() {
    let probe = Probe::read();
    let mut found: Vec<(String, u32)> = probe
        .rows("tsi.has_type")
        .into_iter()
        .map(|fact| {
            let (start, end) = as_span(&fact.args[0]).expect("an occurrence is a span");
            (
                probe.slice(start, end),
                as_id(&fact.args[1]).expect("a has_type names a type"),
            )
        })
        .collect();
    found.sort();
    let bool = probe.named("bool");
    let int64 = probe.named("int64");
    let head = probe.named("Node[int64, string]");
    assert_eq!(
        found,
        [
            ("Flag".to_string(), bool),
            ("Head".to_string(), head),
            ("Limit".to_string(), int64),
        ]
    );
}

/// The builtins carry a class and a name and no origin; `error` and `any`
/// are not among them.
#[test]
fn primitives_carry_a_class_and_no_origin() {
    let probe = Probe::read();
    let classes = probe.classes();
    let mut present: Vec<&str> = classes.values().map(String::as_str).collect();
    present.sort_unstable();
    assert_eq!(
        present,
        ["bool", "byte", "float64", "int", "int64", "string"]
    );
    let names = probe.names();
    for (id, class) in &classes {
        assert!(probe.carries("tsi.type", *id));
        assert!(!probe.has_origin(*id), "primitive {class} carries an origin");
        assert_eq!(&names[id], class);
    }
    let error = probe.named("error");
    assert!(probe.class(error).is_none());
    assert!(probe.has_origin(error));
}

/// Every `tsi.type` id carries a `tsi.name` that spells its origin text, or
/// its class; the go twin of `tests/110_tsi_name.rs`, with `[` for `<`.
#[test]
fn every_named_id_spells_its_origin() {
    let probe = Probe::read();
    let names = probe.names();
    let origins = probe.origin_texts();
    let classes = probe.classes();
    assert!(!names.is_empty(), "no tsi.name row");
    for fact in probe.rows("tsi.type") {
        let id = as_id(&fact.args[0]).expect("tsi.type arg 0 is an id");
        match (names.get(&id), origins.get(&id), classes.get(&id)) {
            (Some(name), _, Some(class)) => {
                assert_eq!(name, class, "primitive {id} names its class");
            }
            (Some(name), Some(origin), None) => {
                assert!(
                    name.ends_with(origin.as_str()) || name.contains(&format!("{origin}[")),
                    "id {id} name `{name}` does not carry origin text `{origin}`"
                );
            }
            (None, Some(origin), None) => panic!("id {id} written `{origin}` has no tsi.name"),
            (None, _, Some(_)) => panic!("primitive {id} has no tsi.name"),
            (Some(_), None, None) => panic!("id {id} named with no origin or class"),
            (None, None, None) => panic!("id {id} has no origin, class or name"),
        }
    }
}
