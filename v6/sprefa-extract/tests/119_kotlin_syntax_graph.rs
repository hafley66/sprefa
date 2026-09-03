//! The kotlin syntax tier's best-guess type graph: what a parse alone claims
//! about `tests/fixtures/tsi/probe_graph.kt` under `--witness --family type`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): on origin/main `42ca51ff6`
//! `extract --witness --family type` emits zero tsi rows for any `.kt` input
//! (`grep -c '"tsi\.' src/lang/kotlin.rs` is 0), so every case below reads an
//! empty fact set and fails on its first assertion.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const FIXTURE: &str = "tests/fixtures/tsi/probe_graph.kt";

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

/// `class Base(val id: Int, label: String) { var label: String; fun ... }`:
/// one edge per property in written order, a bare constructor argument states
/// no edge, methods count their own positions, an extension joins its receiver.
#[test]
fn class_declares_properties_and_reaches_its_methods() {
    let probe = Probe::read();
    let base = probe.id_of("tsi.product", "Base");
    let edges = probe.edges_of(base);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["describe", "id", "label", "render", "size"]);
    let int = probe.named("Int");
    let string = probe.named("String");
    let boolean = probe.named("Boolean");
    assert_eq!(edges["id"], (int, 0));
    assert_eq!(edges["label"], (string, 1));
    let (render, render_at) = edges["render"];
    let (size, size_at) = edges["size"];
    let (describe, describe_at) = edges["describe"];
    assert_eq!((render_at, size_at, describe_at), (0, 1, 2));
    for callable in [render, size, describe] {
        assert!(probe.carries("tsi.callable", callable));
    }
    assert_eq!(probe.origin_text(render), "render");
    assert_eq!(probe.slots("tsi.input", render), [(0, int), (1, boolean)]);
    assert_eq!(probe.slots("tsi.output", render), [(0, string)]);
    assert_eq!(probe.slots("tsi.input", size), []);
    assert_eq!(probe.slots("tsi.output", size), [(0, int)]);
    assert_eq!(probe.slots("tsi.input", describe), []);
    assert_eq!(probe.slots("tsi.output", describe), [(0, string)]);
}

/// A declared class origins at its declaring name, even when a bound or a
/// property written earlier in the file references it first.
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

/// `data class Node<T : Base, K>(val ...) : Base(...), Shape`: an edge to each
/// supertype under its last segment first, then each constructor property and
/// body property; the parameters carry their bounds.
#[test]
fn data_class_edges_its_supertypes_and_declares_parameters() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let base = probe.id_of("tsi.product", "Base");
    let shape = probe.id_of("tsi.product", "Shape");
    let edges = probe.edges_of(node);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(
        labels,
        ["Base", "Shape", "area", "index", "map", "name", "parent", "tags", "value"]
    );
    assert_eq!(edges["Base"], (base, 0));
    assert_eq!(edges["Shape"], (shape, 1));
    let element = probe.parameter(node, 0);
    let key = probe.parameter(node, 1);
    assert_eq!(probe.names()[&element], "T");
    assert_eq!(probe.names()[&key], "K");
    assert_eq!(probe.edges_of(element)["bound"], (base, 0));
    assert!(probe.edges_of(key).is_empty());
    assert_eq!(edges["value"], (element, 2));
    let (tags, _) = edges["tags"];
    assert_eq!(probe.names()[&tags], "List<String>");
    assert_eq!(probe.origin_text(tags), "List");
    assert_eq!(edges["tags"].1, 3);
    let (index, _) = edges["index"];
    assert_eq!(probe.names()[&index], "Map<K, Int>");
    assert_eq!(edges["index"].1, 4);
    assert!(probe.carries("tsi.sum", edges["parent"].0));
    assert_eq!(edges["parent"].1, 5);
    assert_eq!(edges["name"], (probe.named("String"), 6));
    assert_eq!(edges["area"].1, 0);
    assert_eq!(edges["map"].1, 1);
}

/// `L<Int>` written anywhere: one `tsi.called` per distinct written text
/// with the callee being the head and one argument per position.
#[test]
fn written_application_is_called_once() {
    let probe = Probe::read();
    assert_eq!(probe.rows("tsi.called").len(), 8);
    let node = probe.id_of("tsi.product", "Node");
    let shape = probe.id_of("tsi.product", "Shape");
    let int = probe.named("Int");
    let string = probe.named("String");
    let list = probe.named("List");
    assert_eq!(probe.origin_text(list), "List");
    assert!(probe.class(list).is_none());

    let (callee, arguments) = probe.call(probe.named("List<String>"));
    assert_eq!(callee, list);
    assert_eq!(probe.arguments(arguments), [(0, string)]);

    let (callee, arguments) = probe.call(probe.named("Map<K, Int>"));
    assert_eq!(callee, probe.named("Map"));
    assert_eq!(
        probe.arguments(arguments),
        [(0, probe.parameter(node, 1)), (1, int)]
    );

    let concrete = probe.named("Node<Int, String>");
    assert_eq!(probe.origin_text(concrete), "Node");
    let (callee, arguments) = probe.call(concrete);
    assert_eq!(callee, node);
    assert_eq!(probe.arguments(arguments), [(0, int), (1, string)]);

    let (callee, arguments) = probe.call(probe.named("MutableList<Shape>"));
    assert_eq!(callee, probe.named("MutableList"));
    assert_eq!(probe.arguments(arguments), [(0, shape)]);

    let total = probe.id_of("tsi.callable", "total");
    let element = probe.parameter(total, 0);
    let (callee, arguments) = probe.call(probe.named("List<T>"));
    assert_eq!(callee, list);
    assert_eq!(probe.arguments(arguments), [(0, element)]);

    let recursive = probe.named("Comparable<T>");
    assert_eq!(probe.edges_of(element)["bound"], (recursive, 0));
    let (callee, arguments) = probe.call(recursive);
    assert_eq!(callee, probe.named("Comparable"));
    assert_eq!(probe.arguments(arguments), [(0, element)]);
}

/// `T?` is an anonymous sum whose arms are the written type and `null`; every
/// occurrence takes a fresh id, and the mirror of python's `A | None`.
#[test]
fn nullable_is_an_anonymous_sum_with_null() {
    let probe = Probe::read();
    let sums: Vec<u32> = probe
        .rows("tsi.sum")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[0]))
        .collect();
    assert_eq!(sums.len(), 6);
    let base = probe.id_of("tsi.product", "Base");
    let parent = probe.id_of("tsi.sum", "Base?");
    let null = probe.named("null");
    assert_eq!(probe.class(null).as_deref(), Some("null"));
    let edges = probe.edges_of(parent);
    assert_eq!(edges["Base"], (base, 0));
    assert_eq!(edges["null"], (null, 1));
    assert_eq!(edges.len(), 2);

    let head = probe.id_of("tsi.sum", "Node<Int, String>?");
    let edges = probe.edges_of(head);
    assert_eq!(
        edges["Node<Int, String>"],
        (probe.named("Node<Int, String>"), 0)
    );
    assert_eq!(edges["null"], (null, 1));

    let cause = probe.id_of("tsi.sum", "Throwable?");
    let throwable = probe.named("Throwable");
    assert!(probe.class(throwable).is_none());
    assert_eq!(probe.edges_of(cause)["Throwable"], (throwable, 0));
}

/// `sealed class Result { class Ok(...) : Result(); object Empty : Result() }`:
/// a sum with an edge per subclass declared in the file, in written order;
/// each subclass is a product that edges its supertype first.
#[test]
fn sealed_class_is_a_sum_of_its_subclasses() {
    let probe = Probe::read();
    let result = probe.id_of("tsi.sum", "Result");
    assert!(!probe.carries("tsi.product", result));
    let ok = probe.id_of("tsi.product", "Ok");
    let err = probe.id_of("tsi.product", "Err");
    let empty = probe.id_of("tsi.product", "Empty");
    let edges = probe.edges_of(result);
    assert_eq!(edges.len(), 3);
    assert_eq!(edges["Ok"], (ok, 0));
    assert_eq!(edges["Err"], (err, 1));
    assert_eq!(edges["Empty"], (empty, 2));
    let int = probe.named("Int");
    let ok_edges = probe.edges_of(ok);
    assert_eq!(ok_edges["Result"], (result, 0));
    assert_eq!(ok_edges["value"], (int, 1));
    let err_edges = probe.edges_of(err);
    assert_eq!(err_edges["Result"], (result, 0));
    assert_eq!(err_edges["message"], (probe.named("String"), 1));
    assert_eq!(
        err_edges["cause"],
        (probe.id_of("tsi.sum", "Throwable?"), 2)
    );
    assert_eq!(probe.edges_of(empty)["Result"], (result, 0));
}

/// `enum class Color { RED, GREEN, BLUE }`: a sum with an edge per entry, the
/// entry spelled `Color.RED` and originating at its own name.
#[test]
fn enum_class_is_a_sum_of_its_entries() {
    let probe = Probe::read();
    let color = probe.id_of("tsi.sum", "Color");
    let edges = probe.edges_of(color);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["BLUE", "GREEN", "RED"]);
    let (red, at) = edges["RED"];
    assert_eq!(at, 0);
    assert_eq!(probe.names()[&red], "Color.RED");
    assert_eq!(probe.origin_text(red), "RED");
    assert_eq!(edges["GREEN"].1, 1);
    assert_eq!(edges["BLUE"].1, 2);
    assert!(!probe.carries("tsi.product", red));
}

/// `typealias X = T`, `typealias H = (Int, String) -> Unit` and
/// `typealias L<V> = Map<String, V>` are symbols that denote the written type.
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
    let int = probe.named("Int");
    let string = probe.named("String");
    let unit = probe.named("Unit");
    assert_eq!(symbols[0].0, "Handler");
    let handler = probe.id_of("tsi.callable", "(Int, String) -> Unit");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[0].1),
        BTreeSet::from([handler])
    );
    assert_eq!(probe.slots("tsi.input", handler), [(0, int), (1, string)]);
    assert_eq!(probe.slots("tsi.output", handler), [(0, unit)]);
    assert_eq!(symbols[1].0, "Label");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[1].1),
        BTreeSet::from([string])
    );
    assert_eq!(symbols[2].0, "Lookup");
    let lookup = probe.named("Map<String, V>");
    assert_eq!(
        probe.pairs_from("tsi.denotes", symbols[2].1),
        BTreeSet::from([lookup])
    );
    let value = probe.parameter(symbols[2].1, 0);
    assert_eq!(names[&value], "V");
    let (callee, arguments) = probe.call(lookup);
    assert_eq!(callee, probe.named("Map"));
    assert_eq!(probe.arguments(arguments), [(0, string), (1, value)]);
}

/// `fun <R> map(transform: (T) -> R): Node<R, K>?`: the function type is an
/// anonymous callable over the scoped parameters, the result a nullable sum.
#[test]
fn function_type_is_an_anonymous_callable() {
    let probe = Probe::read();
    let node = probe.id_of("tsi.product", "Node");
    let (map, _) = probe.edges_of(node)["map"];
    let element = probe.parameter(node, 0);
    let mapped = probe.parameter(map, 0);
    assert_eq!(probe.names()[&mapped], "R");
    let transform = probe.id_of("tsi.callable", "(T) -> R");
    assert_eq!(probe.slots("tsi.input", map), [(0, transform)]);
    assert_eq!(probe.slots("tsi.input", transform), [(0, element)]);
    assert_eq!(probe.slots("tsi.output", transform), [(0, mapped)]);
    let returned = probe.id_of("tsi.sum", "Node<R, K>?");
    assert_eq!(probe.slots("tsi.output", map), [(0, returned)]);
    let (callee, arguments) = probe.call(probe.named("Node<R, K>"));
    assert_eq!(callee, node);
    assert_eq!(
        probe.arguments(arguments),
        [(0, mapped), (1, probe.parameter(node, 1))]
    );
}

/// A typed top-level property, a constructor property, a body property and a
/// typed parameter all carry the written type at the identifier; a bare
/// `val x = 3` carries nothing.
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
    let int = probe.named("Int");
    let boolean = probe.named("Boolean");
    let byte_array = probe.named("ByteArray");
    assert_eq!(found["limit"], int);
    assert_eq!(found["flag"], boolean);
    assert!(probe.carries("tsi.sum", found["head"]));
    assert_eq!(found["payload"], byte_array);
    assert_eq!(found["wide"], boolean);
    assert_eq!(found["rest"], int);
    assert_eq!(found["width"], int);
    assert_eq!(found["id"], int);
    assert_eq!(found["message"], probe.named("String"));
    assert!(!found.contains_key("loose"));
    assert!(!found.contains_key("T"));
    assert_eq!(probe.rows("tsi.has_type").len(), 25);
}

/// `fun encode(a: A, b: B = x, vararg rest: R): Z`: one input per slot in
/// written order, the return the single output; a bare fun states no slot;
/// a top-level fun with no receiver is reached by no owner.
#[test]
fn fun_declares_inputs_and_an_output() {
    let probe = Probe::read();
    assert_eq!(probe.rows("tsi.callable").len(), 12);
    let encode = probe.id_of("tsi.callable", "encode");
    let byte_array = probe.named("ByteArray");
    let boolean = probe.named("Boolean");
    let int = probe.named("Int");
    assert_eq!(
        probe.slots("tsi.input", encode),
        [(0, byte_array), (1, boolean), (2, int)]
    );
    assert_eq!(probe.slots("tsi.output", encode), [(0, byte_array)]);

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
        [(0, probe.named("List<T>"))]
    );
    assert_eq!(probe.slots("tsi.output", total), [(0, element)]);
    for fact in probe.rows("tsi.edge") {
        let target = as_id(&fact.args[3]).expect("an edge carries a target");
        assert_ne!(target, encode, "an owner reaches encode");
        assert_ne!(target, total, "an owner reaches total");
        assert_ne!(target, missing, "an owner reaches missing");
    }
}

/// `interface Shape` and `object Registry` are products with their property
/// and method edges.
#[test]
fn interface_and_object_are_products() {
    let probe = Probe::read();
    let shape = probe.id_of("tsi.product", "Shape");
    let edges = probe.edges_of(shape);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["area", "name"]);
    let double = probe.named("Double");
    let (area, _) = edges["area"];
    assert_eq!(probe.slots("tsi.input", area), [(0, double)]);
    assert_eq!(probe.slots("tsi.output", area), [(0, double)]);
    assert_eq!(edges["name"], (probe.named("String"), 0));

    let registry = probe.id_of("tsi.product", "Registry");
    let edges = probe.edges_of(registry);
    let labels: Vec<&str> = edges.keys().map(String::as_str).collect();
    assert_eq!(labels, ["register", "shapes"]);
    assert_eq!(edges["shapes"], (probe.named("MutableList<Shape>"), 0));
    let (register, _) = edges["register"];
    assert_eq!(probe.slots("tsi.input", register), [(0, shape)]);
    assert_eq!(probe.slots("tsi.output", register), []);
}

/// The builtins carry a class and a name and no origin; `List`, `Map`,
/// `Throwable` and `ByteArray` are not among them.
#[test]
fn primitives_carry_a_class_and_no_origin() {
    let probe = Probe::read();
    let classes = probe.classes();
    let mut present: Vec<&str> = classes.values().map(String::as_str).collect();
    present.sort_unstable();
    assert_eq!(
        present,
        ["Boolean", "Double", "Int", "String", "Unit", "null"]
    );
    let names = probe.names();
    for (id, class) in &classes {
        assert!(probe.carries("tsi.type", *id));
        assert!(
            !probe.has_origin(*id),
            "primitive {class} carries an origin"
        );
        assert_eq!(&names[id], class);
    }
    for written in ["List", "Map", "Throwable", "ByteArray"] {
        let id = probe.named(written);
        assert!(probe.class(id).is_none());
        assert!(probe.has_origin(id));
    }
}

/// The row count per relation the fixture yields: a change here is a change
/// in what the tier claims.
#[test]
fn relation_counts() {
    let probe = Probe::read();
    let counts: Vec<(&str, usize)> = [
        "tsi.type",
        "tsi.name",
        "tsi.origin",
        "tsi.edge",
        "tsi.product",
        "tsi.sum",
        "tsi.callable",
        "tsi.input",
        "tsi.output",
        "tsi.has_type",
        "tsi.parameter",
        "tsi.called",
        "tsi.argument",
        "tsi.primitive",
        "tsi.symbol",
        "tsi.denotes",
    ]
    .into_iter()
    .map(|relation| (relation, probe.rows(relation).len()))
    .collect();
    assert_eq!(
        counts,
        [
            ("tsi.type", 53),
            ("tsi.name", 56),
            ("tsi.origin", 47),
            ("tsi.edge", 40),
            ("tsi.product", 7),
            ("tsi.sum", 6),
            ("tsi.callable", 12),
            ("tsi.input", 13),
            ("tsi.output", 10),
            ("tsi.has_type", 25),
            ("tsi.parameter", 5),
            ("tsi.called", 8),
            ("tsi.argument", 12),
            ("tsi.primitive", 6),
            ("tsi.symbol", 3),
            ("tsi.denotes", 3),
        ]
    );
}

/// Every `tsi.type` id carries a `tsi.name` that spells its origin text, or
/// its class; the kotlin twin of `tests/110_tsi_name.rs`.
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
                    name.ends_with(origin.as_str()) || name.contains(&format!("{origin}<")),
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
