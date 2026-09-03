//! The python syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.py` under `--witness --family type`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `1b2464c9b`
//! `extract --witness --family type` emits zero tsi rows for any `.py` input
//! (`grep -c '"tsi\.' src/lang/python/_0_source.rs` is 0), so every case
//! below reads an empty fact set and fails on its first assertion.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/tsi/probe_graph.py";

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

/// `class C: a: T; b: U = ...; def m(self, x: X) -> Y`: one edge per annotated
/// field in written order, `self` takes no slot, methods count their own positions.
#[test]
fn class_declares_fields_and_reaches_its_methods() {
    let probe = Probe::read();
    let base = probe.id_of("tsi.product", "Base");
    let edges = probe.edges_of(base);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["id", "label", "render", "size"]);
    let int = probe.named("int");
    let str = probe.named("str");
    let bool = probe.named("bool");
    assert_eq!(edges["id"], (int, 0));
    assert_eq!(edges["label"], (str, 1));
    let (render, render_at) = edges["render"];
    let (size, size_at) = edges["size"];
    assert_eq!((render_at, size_at), (0, 1));
    assert!(probe.carries("tsi.callable", render));
    assert!(probe.carries("tsi.callable", size));
    assert_eq!(probe.origin_text(render), "render");
    assert_eq!(probe.slots("tsi.input", render), [(0, int), (1, bool)]);
    assert_eq!(probe.slots("tsi.output", render), [(0, str)]);
    assert_eq!(probe.slots("tsi.input", size), []);
    assert_eq!(probe.slots("tsi.output", size), [(0, int)]);
}

/// A declared class origins at its declaring name, even when an annotation
/// written earlier in the file references it first.
#[test]
fn declared_class_origins_at_its_own_name() {
    let probe = Probe::read();
    let base = probe.id_of("tsi.product", "Base");
    let declared = probe.offset_of("class Base") + 6;
    assert_eq!(probe.origin(base), (declared, declared + 4));
    let named_base: Vec<u32> = probe
        .names()
        .into_iter()
        .filter(|(_, name)| name == "Base")
        .map(|(id, _)| id)
        .collect();
    assert_eq!(named_base, [base]);
}

/// `class N(Base, Generic[T, K])`: an edge to each base under its own name,
/// one parameter per `Generic` argument, a field of type `T` reaching it, a
/// string annotation reaching the class it spells.
#[test]
fn subclass_edges_its_bases_and_declares_generic_parameters() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let base = probe.id_of("tsi.product", "Base");
    let edges = probe.edges_of(node);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(
        labels,
        ["Base", "index", "pair", "parent", "tags", "twin", "value"]
    );
    assert_eq!(edges["Base"], (base, 0));
    let element = probe.parameter(node, 0);
    let key = probe.parameter(node, 1);
    assert_eq!(probe.names()[&element], "T");
    assert_eq!(probe.names()[&key], "K");
    assert_eq!(edges["value"], (element, 1));
    let (tags, _) = edges["tags"];
    assert_eq!(probe.names()[&tags], "list[str]");
    assert_eq!(probe.origin_text(tags), "list");
    assert_eq!(edges["tags"].1, 2);
    let (index, _) = edges["index"];
    assert_eq!(probe.names()[&index], "dict[K, int]");
    assert_eq!(edges["twin"], (node, 6));
    assert!(!probe.names().values().any(|name| name == "Generic"));
}

/// `L[int]` written anywhere: one `tsi.called` per distinct written text
/// with the callee being the head and one argument per position.
#[test]
fn written_application_is_called_once() {
    let probe = Probe::read();
    assert_eq!(probe.rows("tsi.called").len(), 6);
    let node = probe.id_of("tsi.product", "Node");
    let base = probe.id_of("tsi.product", "Base");
    let int = probe.named("int");
    let str = probe.named("str");
    let list = probe.named("list");
    assert_eq!(probe.origin_text(list), "list");
    assert!(probe.class(list).is_none());

    let (callee, arguments) = probe.call(probe.named("list[str]"));
    assert_eq!(callee, list);
    assert_eq!(probe.arguments(arguments), [(0, str)]);

    let (callee, arguments) = probe.call(probe.named("dict[K, int]"));
    assert_eq!(callee, probe.named("dict"));
    assert_eq!(
        probe.arguments(arguments),
        [(0, probe.parameter(node, 1)), (1, int)]
    );

    let optional = probe.named("Optional[Base]");
    assert_eq!(probe.origin_text(optional), "Optional");
    let (callee, arguments) = probe.call(optional);
    assert_eq!(callee, probe.named("Optional"));
    assert_eq!(probe.arguments(arguments), [(0, base)]);

    let concrete = probe.named("Node[int]");
    assert_eq!(probe.origin_text(concrete), "Node");
    let (callee, arguments) = probe.call(concrete);
    assert_eq!(callee, node);
    assert_eq!(probe.arguments(arguments), [(0, int)]);

    let qualified = probe.named("typing.List[Base]");
    assert_eq!(probe.origin_text(qualified), "List");
    let (callee, arguments) = probe.call(qualified);
    assert_eq!(callee, probe.named("typing.List"));
    assert_eq!(probe.origin_text(callee), "List");
    assert_eq!(probe.arguments(arguments), [(0, base)]);

    let total = probe.id_of("tsi.callable", "total");
    let (callee, arguments) = probe.call(probe.named("list[T]"));
    assert_eq!(callee, list);
    assert_eq!(probe.arguments(arguments), [(0, probe.parameter(total, 0))]);
}

/// `tuple[int, str]` is an anonymous product whose edges are ordinals.
#[test]
fn tuple_is_an_anonymous_product() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let (pair, _) = probe.edges_of(node)["pair"];
    assert!(probe.carries("tsi.product", pair));
    assert_eq!(probe.origin_text(pair), "tuple[int, str]");
    let int = probe.named("int");
    let str = probe.named("str");
    let edges = probe.edges_of(pair);
    assert_eq!(edges["0"], (int, 0));
    assert_eq!(edges["1"], (str, 1));
    assert_eq!(edges.len(), 2);
    assert!(probe
        .rows("tsi.called")
        .iter()
        .all(|fact| as_id(&fact.args[0]) != Some(pair)));
}

/// `A | None` is an anonymous sum whose edges are labelled by the written arms.
#[test]
fn union_is_an_anonymous_sum() {
    let probe = Probe::read();
    let sums: Vec<u32> = probe
        .rows("tsi.sum")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[0]))
        .collect();
    assert_eq!(sums.len(), 1);
    let head = sums[0];
    assert_eq!(probe.origin_text(head), "Node[int] | None");
    let edges = probe.edges_of(head);
    let none = probe.named("None");
    assert_eq!(probe.class(none).as_deref(), Some("None"));
    assert_eq!(edges["Node[int]"], (probe.named("Node[int]"), 0));
    assert_eq!(edges["None"], (none, 1));
    assert_eq!(edges.len(), 2);
}

/// `X = str`, `X = typing.List[Base]` and `type X = int` are symbols that
/// denote; a `TypeVar` assignment and a plain value are not.
#[test]
fn alias_denotes_the_written_type() {
    let probe = Probe::read();
    let names = probe.names();
    let mut symbols: Vec<(String, u32)> = probe
        .rows("tsi.symbol")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[0]))
        .map(|id| (names[&id].clone(), id))
        .collect();
    symbols.sort();
    assert_eq!(symbols.len(), 3);
    for (_, symbol) in &symbols {
        assert!(!probe.carries("tsi.type", *symbol));
    }
    let str = probe.named("str");
    let int = probe.named("int");
    let handle = probe.named("typing.List[Base]");
    assert_eq!(symbols[0].0, "Handle");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[0].1),
        BTreeSet::from([handle])
    );
    assert_eq!(symbols[1].0, "Label");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[1].1),
        BTreeSet::from([str])
    );
    assert_eq!(symbols[2].0, "Meters");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[2].1),
        BTreeSet::from([int])
    );
}

/// `x: T = ...` at module level, an annotated field and an annotated
/// parameter all carry the written type at the identifier; a bare `x = 3`
/// carries nothing.
#[test]
fn typed_values_carry_has_type() {
    let probe = Probe::read();
    let found: BTreeMap<String, u32> = probe
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
    let int = probe.named("int");
    let bool = probe.named("bool");
    let bytes = probe.named("bytes");
    assert_eq!(found["LIMIT"], int);
    assert_eq!(found["flag"], bool);
    assert!(probe.carries("tsi.sum", found["head"]));
    assert_eq!(found["payload"], bytes);
    assert_eq!(found["width"], int);
    assert_eq!(found["title"], probe.named("str"));
    assert!(!found.contains_key("loose"));
    assert!(!found.contains_key("T"));
    assert_eq!(probe.rows("tsi.has_type").len(), 23);
}

/// `def f(a: A, b: B = x, *rest: R, **options: O) -> Z`: one input per slot
/// in written order, the return the single output; a bare def states no slot;
/// a module `TypeVar` a def names is that def's parameter.
#[test]
fn def_declares_inputs_and_an_output() {
    let probe = Probe::read();
    assert_eq!(probe.rows("tsi.callable").len(), 6);
    let encode = probe.id_of("tsi.callable", "encode");
    let bytes = probe.named("bytes");
    let bool = probe.named("bool");
    let int = probe.named("int");
    let str = probe.named("str");
    assert_eq!(
        probe.slots("tsi.input", encode),
        [(0, bytes), (1, bool), (2, int), (3, str)]
    );
    assert_eq!(probe.slots("tsi.output", encode), [(0, bytes)]);

    let missing = probe.id_of("tsi.callable", "missing");
    assert_eq!(probe.slots("tsi.input", missing), []);
    assert_eq!(probe.slots("tsi.output", missing), []);

    let total = probe.id_of("tsi.callable", "total");
    let element = probe.parameter(total, 0);
    assert_eq!(probe.names()[&element], "T");
    assert_ne!(
        element,
        probe.parameter(probe.id_of("tsi.product", "Node"), 0)
    );
    assert_eq!(
        probe.slots("tsi.input", total),
        [(0, probe.named("list[T]"))]
    );
    assert_eq!(probe.slots("tsi.output", total), [(0, element)]);
    for fact in probe.rows("tsi.edge") {
        let target = as_id(&fact.args[3]).expect("an edge carries a target");
        assert_ne!(target, encode, "an owner reaches encode");
        assert_ne!(target, total, "an owner reaches total");
    }
}

/// `NamedTuple`, `TypedDict` and `typing.Protocol` bases are edges like any
/// other base; the class stays a product with its annotated members.
#[test]
fn library_bases_are_edges_on_a_product() {
    let probe = Probe::read();
    let point = probe.id_of("tsi.product", "Point");
    let edges = probe.edges_of(point);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["NamedTuple", "x", "y"]);
    let float = probe.named("float");
    assert_eq!(edges["x"], (float, 1));
    assert_eq!(edges["y"], (float, 2));
    let (named_tuple, _) = edges["NamedTuple"];
    assert_eq!(probe.names()[&named_tuple], "NamedTuple");
    assert!(probe.class(named_tuple).is_none());

    let movie = probe.id_of("tsi.product", "Movie");
    let labels: Vec<String> = probe.edges_of(movie).keys().cloned().collect();
    assert_eq!(labels, ["TypedDict", "title", "year"]);

    let shape = probe.id_of("tsi.product", "Shape");
    let edges = probe.edges_of(shape);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["Protocol", "area"]);
    let (protocol, _) = edges["Protocol"];
    assert_eq!(probe.names()[&protocol], "typing.Protocol");
    assert_eq!(probe.origin_text(protocol), "Protocol");
    let (area, _) = edges["area"];
    assert_eq!(probe.slots("tsi.input", area), [(0, float)]);
    assert_eq!(probe.slots("tsi.output", area), [(0, float)]);
}

/// The builtins carry a class and a name and no origin; `list`, `Optional`
/// and `NamedTuple` are not among them.
#[test]
fn primitives_carry_a_class_and_no_origin() {
    let probe = Probe::read();
    let classes = probe.classes();
    let mut present: Vec<&str> = classes.values().map(String::as_str).collect();
    present.sort_unstable();
    assert_eq!(present, ["None", "bool", "bytes", "float", "int", "str"]);
    let names = probe.names();
    for (id, class) in &classes {
        assert!(probe.carries("tsi.type", *id));
        assert!(
            !probe.has_origin(*id),
            "primitive {class} carries an origin"
        );
        assert_eq!(&names[id], class);
    }
    for written in ["list", "Optional", "NamedTuple"] {
        let id = probe.named(written);
        assert!(probe.class(id).is_none());
        assert!(probe.has_origin(id));
    }
}

/// Every `tsi.type` id carries a `tsi.name` that spells its origin text, or
/// its class; the python twin of `tests/110_tsi_name.rs`, with `[` for `<`.
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
