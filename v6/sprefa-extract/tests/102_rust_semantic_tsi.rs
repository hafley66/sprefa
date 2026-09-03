//! The rust SEMANTIC tier: the rust-analyzer item walk enumerates whole
//! relations rather than answering one reference site, and says which ones it
//! enumerated exhaustively.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha 5a2cef0ff, whole file): the same
//! command emitted 14 rows carrying the semantic run row
//!   {"record":"run","run":1,"mode":"semantic","tool":"rust-analyzer",...}
//! and ZERO `checker_walk` witnesses, zero `record=fact` rows and zero coverage
//! rows on run 1, so every case below sees an empty semantic fact set.
//! `the_walk_is_off_without_witness` guards the other direction;
//! `cargo test --test 94_rust_checker_types` is the tier's own twin.

#![cfg(feature = "rust-checker")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const ROOT: &str = "tests/fixtures/tsi/rust_probe";
const PROBE: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

/// The relations the walk enumerates exhaustively over this fixture. A relation
/// the fixture never exercises gets no claim at all: `complete` over an empty
/// relation is a producer defect the reverse door rejects.
const COMPLETE: &[&str] = &[
    "rust.assoc",
    "rust.impl",
    "rust.lifetime",
    "rust.ownership",
    "rust.trait",
    "tsi.argument",
    "tsi.callable",
    "tsi.called",
    "tsi.denotes",
    "tsi.input",
    "tsi.origin",
    "tsi.output",
    "tsi.parameter",
    "tsi.primitive",
    "tsi.product",
    "tsi.sum",
    "tsi.type",
];

/// Every relation the walk samples rather than enumerates, and nothing else.
const PARTIAL: &[&str] = &[
    "tsi.assignable",
    "tsi.conforms",
    "tsi.edge",
    "tsi.equivalent",
    "tsi.has_type",
    "tsi.subtype",
];

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

/// One witnessed, checker-driven resolve over the fixture, with the fixture's
/// own bytes beside it: an origin span is what names an id in a failure print.
struct Walk {
    stream: String,
    rows: Vec<FlatFact>,
    facts: Vec<FactOut>,
    source: Vec<u8>,
}

impl Walk {
    fn read() -> Self {
        let stream = extract(&[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            ROOT,
            "--rust-checker",
            PROBE,
        ]);
        let rows: Vec<FlatFact> = stream
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str(line)
                    .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"))
            })
            .collect();
        let facts = rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Fact(fact) => Some(fact.clone()),
                _ => None,
            })
            .collect();
        let source = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(PROBE))
            .expect("the fixture is readable");
        Self {
            stream,
            rows,
            facts,
            source,
        }
    }

    fn rows_of(&self, wanted: &str) -> Vec<&FactOut> {
        self.facts
            .iter()
            .filter(|fact| fact.relation == wanted)
            .collect()
    }

    fn semantic_run(&self) -> u32 {
        let mut found: Vec<u32> = self
            .rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Run(run) => (run.tool == "rust-analyzer").then_some(run.run),
                _ => None,
            })
            .collect();
        assert_eq!(found.len(), 1, "one rust-analyzer run per stream");
        found.remove(0)
    }

    /// (run, relation) -> complete, over every coverage row in the stream.
    fn coverage(&self) -> BTreeMap<(u32, String), bool> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Coverage(claim) => {
                    Some(((claim.run, claim.relation.clone()), claim.complete))
                }
                _ => None,
            })
            .collect()
    }

    fn diagnostics(&self) -> BTreeMap<(u32, String), String> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Diagnostic(row) => {
                    Some(((row.run, row.relation.clone()), row.detail.clone()))
                }
                _ => None,
            })
            .collect()
    }

    /// The text a span covers. A corpus span slices the fixture; a declaration
    /// outside the supplied files keeps its own path and is read from disk.
    fn text_at(&self, arg: &Arg) -> Option<String> {
        let Arg::Span(key, start, end) = arg else {
            return None;
        };
        let (start, end) = (*start as usize, *end as usize);
        if end <= start {
            return None;
        }
        if key.starts_with("blake3:") {
            let slice = self.source.get(start..end)?;
            return Some(String::from_utf8_lossy(slice).to_string());
        }
        if !Path::new(key).is_absolute() {
            return None;
        }
        let bytes = std::fs::read(key).ok()?;
        Some(String::from_utf8_lossy(bytes.get(start..end)?).to_string())
    }

    /// Every id whose origin covers exactly `written`, in the fixture or in the
    /// std source a leaf type was declared in.
    fn ids_named(&self, written: &str) -> BTreeSet<u32> {
        self.rows_of("tsi.origin")
            .into_iter()
            .filter(|fact| self.text_at(&fact.args[2]).as_deref() == Some(written))
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect()
    }

    /// The one id written `name` that a declaration minted. An application of
    /// the same constructor origins at the same range and carries a call row.
    fn declared_named(&self, written: &str) -> u32 {
        let applied: BTreeSet<u32> = self
            .rows_of("tsi.called")
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect();
        let mut found: Vec<u32> = self
            .ids_named(written)
            .into_iter()
            .filter(|id| !applied.contains(id))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{written} named {} declarations",
            found.len()
        );
        found.remove(0)
    }

    fn edge(&self, owner: u32, label: &str) -> &FactOut {
        let mut found: Vec<&FactOut> = self
            .edges_of(owner)
            .into_iter()
            .filter(|fact| as_text(&fact.args[2]) == Some(label))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "owner {owner} has {} `{label}` edges",
            found.len()
        );
        found.remove(0)
    }

    fn edges_of(&self, owner: u32) -> Vec<&FactOut> {
        self.rows_of("tsi.edge")
            .into_iter()
            .filter(|fact| as_id(&fact.args[1]) == Some(owner))
            .collect()
    }

    fn carries(&self, wanted: &str, id: u32) -> bool {
        self.rows_of(wanted)
            .into_iter()
            .any(|fact| as_id(&fact.args[0]) == Some(id))
    }

    /// The word `rust.ownership` files under one edge.
    fn ownership(&self, edge: &FactOut) -> String {
        let edge = as_id(&edge.args[0]).expect("an edge declares its own id");
        let mut found: Vec<String> = self
            .rows_of("rust.ownership")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(edge))
            .filter_map(|fact| as_atom(&fact.args[1]).map(str::to_string))
            .collect();
        assert_eq!(found.len(), 1, "edge {edge} carries {} words", found.len());
        found.remove(0)
    }

    /// The `tsi.input` or `tsi.output` a callable declares at `position`.
    fn slot(&self, relation: &str, callable: u32, position: i64) -> u32 {
        let mut found: Vec<u32> = self
            .rows_of(relation)
            .into_iter()
            .filter(|fact| {
                as_id(&fact.args[0]) == Some(callable) && as_int(&fact.args[1]) == Some(position)
            })
            .filter_map(|fact| as_id(&fact.args[2]))
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{relation} of {callable} at {position} named {} types",
            found.len()
        );
        found.remove(0)
    }

    /// The one application of `constructor`, with its argument list resolved.
    fn application(&self, result: u32) -> (u32, Vec<u32>) {
        let mut found: Vec<&FactOut> = self
            .rows_of("tsi.called")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(result))
            .collect();
        assert_eq!(found.len(), 1, "{result} is applied {} times", found.len());
        let call = found.remove(0);
        let list = as_id(&call.args[2]).expect("a call names its argument list");
        let mut arguments: Vec<(i64, u32)> = self
            .rows_of("tsi.argument")
            .into_iter()
            .filter(|fact| as_id(&fact.args[0]) == Some(list))
            .filter_map(|fact| Some((as_int(&fact.args[1])?, as_id(&fact.args[2])?)))
            .collect();
        arguments.sort();
        (
            as_id(&call.args[1]).expect("a call names its constructor"),
            arguments.into_iter().map(|(_, id)| id).collect(),
        )
    }

    /// Every id a `tsi.type`, `tsi.symbol`, `tsi.edge`, `rust.impl` or
    /// `tsi.called` row declares, which is the closure the reverse door checks.
    fn declared(&self) -> BTreeSet<u32> {
        let mut declared: BTreeSet<u32> = BTreeSet::new();
        for fact in &self.facts {
            let position = match fact.relation.as_str() {
                "tsi.type" | "tsi.symbol" | "tsi.value" | "tsi.edge" | "rust.impl" => 0,
                "tsi.called" => 2,
                _ => continue,
            };
            if let Some(id) = as_id(&fact.args[position]) {
                declared.insert(id);
            }
        }
        declared
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

/// Criterion 5: the walk is a run of its own, and every row it produced says so.
#[test]
fn the_walk_is_one_run_and_every_row_witnesses_it() {
    let walk = Walk::read();
    let mut runs: Vec<(u32, String, String)> = walk
        .rows
        .iter()
        .filter_map(|row| match row {
            FlatFact::Run(run) => Some((
                run.run,
                serde_json::to_value(run.mode)
                    .expect("a mode is a word")
                    .as_str()
                    .expect("a mode is a word")
                    .to_string(),
                run.tool.clone(),
            )),
            _ => None,
        })
        .collect();
    runs.sort();
    assert_eq!(
        runs,
        vec![
            (0, "syntax".to_string(), "extract".to_string()),
            (1, "semantic".to_string(), "rust-analyzer".to_string()),
        ]
    );

    let semantic = walk.semantic_run();
    let ordinals: BTreeSet<u32> = walk.facts.iter().map(|fact| fact.fact).collect();
    assert!(!ordinals.is_empty(), "the walk produced no fact rows");
    let mut walked: BTreeSet<u32> = BTreeSet::new();
    for row in &walk.rows {
        let FlatFact::Witness(witness) = row else {
            continue;
        };
        if !matches!(
            serde_json::to_value(witness.method).expect("a method is a word"),
            serde_json::Value::String(ref word) if word == "checker_walk"
        ) {
            continue;
        }
        assert_eq!(
            witness.run, semantic,
            "a checker walk names the rust-analyzer run"
        );
        walked.insert(witness.fact);
    }
    assert_eq!(walked, ordinals, "one checker_walk witness per walked fact");
}

/// Criterion 6, the rust half: the checker reads the shape a parse can only
/// guess at, field by field, with each field's own type behind it.
#[test]
fn a_product_carries_named_positioned_typed_fields() {
    let walk = Walk::read();
    let user = walk.declared_named("User");
    assert!(walk.carries("tsi.product", user));

    let parameter = walk
        .rows_of("tsi.parameter")
        .into_iter()
        .find(|fact| as_id(&fact.args[1]) == Some(user))
        .expect("the struct declares one type parameter");
    assert_eq!(as_int(&parameter.args[2]), Some(0));
    // rust-analyzer exposes no variance, so a stated one would be invented.
    assert_eq!(as_atom(&parameter.args[3]), Some("unspecified"));
    let element = as_id(&parameter.args[0]).expect("a parameter is a type");
    assert!(walk.ids_named("T").contains(&element));

    let id_edge = walk.edge(user, "id");
    assert_eq!(as_int(&id_edge.args[4]), Some(0));
    assert_eq!(as_id(&id_edge.args[3]), Some(element));

    let name_edge = walk.edge(user, "name");
    assert_eq!(as_int(&name_edge.args[4]), Some(1));
    let optional = as_id(&name_edge.args[3]).expect("an edge names its target");
    let (constructor, arguments) = walk.application(optional);
    assert_eq!(constructor, walk.declared_named("Option"));
    assert_eq!(arguments, vec![walk.declared_named("String")]);
}

/// The claim no parse can make: an associated type is declared by one item and
/// answered by another, and only the checker knows which answer belongs to whom.
#[test]
fn an_associated_type_is_declared_by_a_trait_and_answered_by_an_impl() {
    let walk = Walk::read();
    let mapper = walk.declared_named("Mapper");
    let user = walk.declared_named("User");
    assert!(walk.carries("rust.trait", mapper));

    let stated: Vec<&FactOut> = walk
        .rows_of("rust.assoc")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(mapper))
        .collect();
    assert_eq!(stated.len(), 1, "the trait declares one associated type");
    assert_eq!(as_text(&stated[0].args[1]), Some("Output"));

    let mut implemented: Vec<&FactOut> = walk
        .rows_of("rust.impl")
        .into_iter()
        .filter(|fact| as_id(&fact.args[1]) == Some(user) && as_id(&fact.args[2]) == Some(mapper))
        .collect();
    assert_eq!(implemented.len(), 1, "the fixture writes one impl");
    implemented.remove(0);

    let conforms: Vec<&FactOut> = walk
        .rows_of("tsi.conforms")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(user))
        .collect();
    assert_eq!(conforms.len(), 1);
    assert_eq!(as_id(&conforms[0].args[1]), Some(mapper));
    assert_eq!(as_atom(&conforms[0].args[2]), Some("declared"));

    let answered: Vec<&FactOut> = walk
        .rows_of("rust.assoc")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(user))
        .collect();
    assert_eq!(answered.len(), 1, "the impl answers one associated type");
    assert_eq!(as_text(&answered[0].args[1]), Some("Output"));
    let produced = as_id(&answered[0].args[2]).expect("an assoc names its target");
    let (constructor, arguments) = walk.application(produced);
    assert_eq!(constructor, walk.declared_named("Vec"));
    assert!(walk.ids_named("T").contains(&arguments[0]));

    let semantic = walk.semantic_run();
    assert_eq!(
        walk.coverage().get(&(semantic, "tsi.conforms".to_string())),
        Some(&false)
    );
    assert_eq!(
        walk.diagnostics()
            .get(&(semantic, "tsi.conforms".to_string()))
            .map(String::as_str),
        Some("declared impls only; blanket and auto traits not enumerated")
    );
}

/// The two things rust spells that no other type system does, and a type whose
/// own shape names itself: the second visit reads the id the first minted.
#[test]
fn a_lifetime_and_an_ownership_word_ride_every_field() {
    let walk = Walk::read();
    let view = walk.declared_named("View");
    let lifetimes: Vec<&str> = walk
        .rows_of("rust.lifetime")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(view))
        .filter_map(|fact| as_atom(&fact.args[1]))
        .collect();
    assert_eq!(lifetimes, vec!["a"]);

    let borrowed = walk.edge(view, "text");
    assert_eq!(walk.ownership(borrowed), "shared");
    let element = as_id(&borrowed.args[3]).expect("an edge names its target");
    let primitive = walk
        .rows_of("tsi.primitive")
        .into_iter()
        .find(|fact| as_id(&fact.args[0]) == Some(element))
        .expect("a borrow targets its pointee");
    assert_eq!(as_atom(&primitive.args[1]), Some("str"));

    assert_eq!(walk.ownership(walk.edge(view, "owned")), "owned");

    let shared = walk.edge(view, "shared");
    assert_eq!(walk.ownership(shared), "shared");
    assert_eq!(
        as_id(&shared.args[3]),
        Some(view),
        "the edge closes the cycle through the id the declaration minted"
    );
}

/// A sum is its variants, and a variant is a product of its own: the shape the
/// parse writes as one word.
#[test]
fn a_sum_carries_one_product_per_variant() {
    let walk = Walk::read();
    let shape = walk.declared_named("Shape");
    assert!(walk.carries("tsi.sum", shape));
    assert!(!walk.carries("tsi.product", shape));

    let mut variants: Vec<(i64, String)> = walk
        .edges_of(shape)
        .into_iter()
        .filter_map(|fact| Some((as_int(&fact.args[4])?, as_text(&fact.args[2])?.to_string())))
        .collect();
    variants.sort();
    assert_eq!(
        variants,
        vec![(0, "Circle".to_string()), (1, "Square".to_string())]
    );

    let square = as_id(&walk.edge(shape, "Square").args[3]).expect("a variant edge has a target");
    assert!(walk.carries("tsi.product", square));
    let side = as_id(&walk.edge(square, "side").args[3]).expect("a field edge has a target");
    let primitive = walk
        .rows_of("tsi.primitive")
        .into_iter()
        .find(|fact| as_id(&fact.args[0]) == Some(side))
        .expect("the field's type is a primitive");
    assert_eq!(as_atom(&primitive.args[1]), Some("f64"));

    // A tuple variant's fields are named by position, so the label is `0`.
    let circle = as_id(&walk.edge(shape, "Circle").args[3]).expect("a variant edge has a target");
    assert_eq!(as_int(&walk.edge(circle, "0").args[4]), Some(0));
}

/// A callable's inputs and output are types, never names, so a signature the
/// parse copied verbatim becomes a shape a consumer can follow.
#[test]
fn a_callable_carries_its_inputs_and_its_output() {
    let walk = Walk::read();
    let declared = walk.declared();
    let named = walk.ids_named("map");
    assert_eq!(
        named.len(),
        2,
        "the trait declares `map` and the impl answers it"
    );
    for map in named {
        assert!(walk.carries("tsi.callable", map));
        let element = walk.slot("tsi.input", map, 0);
        assert!(
            walk.ids_named("T").contains(&element),
            "`&self` is not an input, so the one parameter is at position 0"
        );
        let produced = walk.slot("tsi.output", map, 0);
        assert!(declared.contains(&produced), "the output names a live id");
    }
}

/// Criterion 5: `complete` means absence from the relation is meaningful, so the
/// set of relations claiming it is the contract a consumer reads.
#[test]
fn the_complete_claims_are_exactly_the_enumerated_relations() {
    let walk = Walk::read();
    let semantic = walk.semantic_run();
    let coverage = walk.coverage();

    let complete: BTreeSet<&str> = coverage
        .iter()
        .filter(|((run, _), complete)| *run == semantic && **complete)
        .map(|((_, relation), _)| relation.as_str())
        .collect();
    assert_eq!(
        complete,
        COMPLETE.iter().copied().collect::<BTreeSet<&str>>()
    );

    let partial: BTreeSet<&str> = coverage
        .iter()
        .filter(|((run, _), complete)| *run == semantic && !**complete)
        .map(|((_, relation), _)| relation.as_str())
        .collect();
    assert_eq!(partial, PARTIAL.iter().copied().collect::<BTreeSet<&str>>());

    let diagnostics = walk.diagnostics();
    for relation in PARTIAL {
        assert!(
            diagnostics.contains_key(&(semantic, relation.to_string())),
            "{relation} is partial with no diagnostic beside it"
        );
    }

    // A `complete` claim over an empty relation would say absence is meaningful
    // about a relation the walk never reached.
    let emitted: BTreeSet<&str> = walk
        .facts
        .iter()
        .map(|fact| fact.relation.as_str())
        .collect();
    for relation in &complete {
        assert!(emitted.contains(relation), "{relation} claims an empty set");
    }
    assert!(
        !coverage
            .iter()
            .any(|((run, _), complete)| *run == 0 && *complete),
        "the syntax run enumerates nothing"
    );
}

/// The reverse door's id closure, run on the producer's own side: an id no row
/// declares would leave a v7 import chasing a node that is not there.
#[test]
fn every_id_is_declared_and_the_stream_survives_the_door() {
    let walk = Walk::read();
    let declared = walk.declared();
    for fact in &walk.facts {
        for arg in &fact.args {
            if let Some(id) = as_id(arg) {
                assert!(
                    declared.contains(&id),
                    "{} names undeclared id {id}",
                    fact.relation
                );
            }
        }
    }

    let scratch = std::env::temp_dir().join("sprefa_a6_rust_semantic");
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let raw = scratch.join("stream.jsonl");
    std::fs::write(&raw, &walk.stream).expect("write the stream");
    let once = extract(&["--ingest", raw.to_str().expect("utf8 path")]);
    let canonical = scratch.join("once.jsonl");
    std::fs::write(&canonical, &once).expect("write the canonical form");
    let twice = extract(&["--ingest", canonical.to_str().expect("utf8 path")]);
    assert_eq!(once, twice, "the reverse door is idempotent");
}

/// The cost cap: the walk is the tier's expensive half and answers no resolve
/// site, so a stream with no envelope to carry it never pays for it.
#[test]
fn the_walk_is_off_without_witness() {
    let stream = extract(&[
        "--resolve",
        "--family",
        "type",
        "--project-root",
        ROOT,
        "--rust-checker",
        PROBE,
    ]);
    for line in stream.lines().filter(|line| !line.trim().is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).expect("a row is JSON");
        assert_ne!(row["record"], "fact", "{line}");
        assert_ne!(row["record"], "coverage", "{line}");
        assert!(row.get("fact").is_none(), "{line}");
    }
}
