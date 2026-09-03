//! The ts SEMANTIC tier: the tsc walk enumerates whole relations rather than
//! answering one reference site, and says which ones it enumerated exhaustively.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha b9ecef38a, whole file): the same
//! command emitted the A2 semantic run row
//!   {"record":"run","run":1,"mode":"semantic","tool":"tsc",...}
//! and ZERO `checker_walk` witnesses, zero `record=fact` rows and zero coverage
//! rows on run 1 (`--witness --resolve` carried `extract.call` and
//! `extract.type` partial on run 0 and nothing else), so every case below sees
//! an empty semantic fact set. `the_walk_is_off_without_witness` guards the
//! other direction; `cargo test --test 92_ts_checker` is the tier's own twin.

#![cfg(feature = "ts-checker")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const DIR: &str = "tests/fixtures/tsi";
const PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RECURSIVE: &str = "tests/fixtures/tsi/recursive.ts";

/// The relations the walk enumerates exhaustively over this fixture. A relation
/// the fixture never exercises gets no claim at all: `complete` over an empty
/// relation is a producer defect the reverse door rejects.
const COMPLETE: &[&str] = &[
    "ts.interface",
    "ts.mapped",
    "ts.optional",
    "ts.readonly",
    "tsi.argument",
    "tsi.callable",
    "tsi.called",
    "tsi.denotes",
    "tsi.has_type",
    "tsi.input",
    "tsi.origin",
    "tsi.name",
    "tsi.output",
    "tsi.parameter",
    "tsi.primitive",
    "tsi.product",
    "tsi.symbol",
    "tsi.type",
];

/// Every relation the walk samples rather than enumerates, and nothing else.
const PARTIAL: &[&str] = &[
    "tsi.assignable",
    "tsi.conforms",
    "tsi.edge",
    "tsi.equivalent",
    "tsi.subtype",
];

/// A `typescript` the driver can load, the way `tests/92_ts_checker.rs` finds
/// one: a checkout's `lib/typescript.js` is the built compiler.
fn typescript() -> String {
    if let Ok(pinned) = std::env::var("SPREFA_TS_CHECKER_TYPESCRIPT") {
        return pinned;
    }
    let root = std::env::var("RATCHET_TS_ROOT")
        .unwrap_or_else(|_| "/Users/chrishafley/projects/TypeScript-5.9".to_string());
    let built = PathBuf::from(&root).join("lib/typescript.js");
    assert!(
        built.is_file(),
        "no typescript for the checker tier: set SPREFA_TS_CHECKER_TYPESCRIPT to a \
         typescript.js, or RATCHET_TS_ROOT to a TypeScript checkout (tried {})",
        built.display()
    );
    built.to_string_lossy().into_owned()
}

fn extract(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SPREFA_TS_CHECKER_TYPESCRIPT", typescript())
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

/// One witnessed, checker-driven resolve over one fixture, with the fixture's
/// own bytes beside it: an origin span is what names an id in a failure print.
struct Walk {
    stream: String,
    rows: Vec<FlatFact>,
    facts: Vec<FactOut>,
    source: Vec<u8>,
}

impl Walk {
    fn read(fixture: &str) -> Self {
        let stream = extract(&[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            DIR,
            "--ts-checker",
            fixture,
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
        let source = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture))
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
                FlatFact::Run(run) => (run.tool == "tsc").then_some(run.run),
                _ => None,
            })
            .collect();
        assert_eq!(found.len(), 1, "one tsc run per stream");
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

    /// The text a corpus span covers. A span naming a file off the corpus keeps
    /// the file's own path where the digest goes, and slices nothing.
    fn text_at(&self, arg: &Arg) -> Option<String> {
        let Arg::Span(digest, start, end) = arg else {
            return None;
        };
        if !digest.starts_with("blake3:") {
            return None;
        }
        Some(String::from_utf8_lossy(&self.source[*start as usize..*end as usize]).to_string())
    }

    /// Every id whose origin covers exactly `written` in the fixture.
    fn ids_named(&self, written: &str) -> BTreeSet<u32> {
        self.rows_of("tsi.origin")
            .into_iter()
            .filter(|fact| self.text_at(&fact.args[2]).as_deref() == Some(written))
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect()
    }

    /// The one id written `name` that declares a type parameter of its own: the
    /// GENERIC, never one of its applications, which origin at the same name.
    fn generic_named(&self, written: &str) -> u32 {
        let callees: BTreeSet<u32> = self
            .rows_of("tsi.parameter")
            .into_iter()
            .filter_map(|fact| as_id(&fact.args[1]))
            .collect();
        let mut found: Vec<u32> = self
            .ids_named(written)
            .into_iter()
            .filter(|id| callees.contains(id))
            .collect();
        assert_eq!(found.len(), 1, "{written} named {} generics", found.len());
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
    let walk = Walk::read(PROBE);
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
            (1, "semantic".to_string(), "tsc".to_string()),
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
        assert_eq!(witness.run, semantic, "a checker walk names the tsc run");
        walked.insert(witness.fact);
    }
    assert_eq!(walked, ordinals, "one checker_walk witness per walked fact");
}

/// Criterion 6, the ts half: the checker reads the shape a parse can only guess
/// at, field by field, with each field's own type behind it.
#[test]
fn a_product_carries_named_positioned_typed_fields() {
    let walk = Walk::read(PROBE);
    let user = walk.generic_named("User");
    assert!(walk.carries("tsi.product", user));

    let id_edge = walk.edge(user, "id");
    assert_eq!(as_int(&id_edge.args[4]), Some(0));
    let id_edge_id = as_id(&id_edge.args[0]).expect("an edge declares its own id");
    assert!(walk.carries("ts.readonly", id_edge_id));
    assert!(!walk.carries("ts.optional", id_edge_id));

    let name_edge = walk.edge(user, "name");
    assert_eq!(as_int(&name_edge.args[4]), Some(1));
    let name_edge_id = as_id(&name_edge.args[0]).expect("an edge declares its own id");
    assert!(walk.carries("ts.optional", name_edge_id));
    assert!(!walk.carries("ts.readonly", name_edge_id));

    let string_type = as_id(&name_edge.args[3]).expect("an edge names its target");
    let primitive = walk
        .rows_of("tsi.primitive")
        .into_iter()
        .find(|fact| as_id(&fact.args[0]) == Some(string_type))
        .expect("the field's type is a primitive");
    assert_eq!(as_atom(&primitive.args[1]), Some("string"));
}

/// The syntax tier spells a written `Name<Args>` and stops. The checker names
/// the type the application COMPUTES and puts it at the occurrence.
#[test]
fn a_generic_argument_is_a_call_with_its_own_result_type() {
    let walk = Walk::read(PROBE);
    let user = walk.generic_named("User");
    let mut applied: Vec<&FactOut> = walk
        .rows_of("tsi.called")
        .into_iter()
        .filter(|fact| as_id(&fact.args[1]) == Some(user))
        .collect();
    assert_eq!(applied.len(), 1, "`User` is applied once in the fixture");
    let call = applied.remove(0);
    let result = as_id(&call.args[0]).expect("a call names its result");
    let list = as_id(&call.args[2]).expect("a call names its argument list");

    let mut arguments: Vec<&FactOut> = walk
        .rows_of("tsi.argument")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(list))
        .collect();
    assert_eq!(arguments.len(), 1, "`this` is not a written argument");
    let argument = arguments.remove(0);
    assert_eq!(as_int(&argument.args[1]), Some(0));
    let number = as_id(&argument.args[2]).expect("an argument names a type");
    let primitive = walk
        .rows_of("tsi.primitive")
        .into_iter()
        .find(|fact| as_id(&fact.args[0]) == Some(number))
        .expect("the argument is a primitive");
    assert_eq!(as_atom(&primitive.args[1]), Some("number"));

    let occurrences: Vec<String> = walk
        .rows_of("tsi.has_type")
        .into_iter()
        .filter(|fact| as_id(&fact.args[1]) == Some(result))
        .filter_map(|fact| walk.text_at(&fact.args[0]))
        .collect();
    assert_eq!(
        occurrences,
        vec!["User<number>".to_string()],
        "the application's own range carries its computed type"
    );
}

/// The claim the syntax tier cannot make: a computed type's whole shape, every
/// field of it optional because the mapping said so.
#[test]
fn a_mapped_type_carries_its_parts_and_only_optional_fields() {
    let walk = Walk::read(PROBE);
    let mapped = walk.rows_of("ts.mapped");
    assert_eq!(mapped.len(), 1, "the fixture writes one mapped type");
    let query = as_id(&mapped[0].args[0]).expect("a mapped type names itself");
    for part in &mapped[0].args[1..] {
        assert!(
            as_id(part).is_some(),
            "the key parameter, constraint and template are all ids"
        );
    }

    let user = walk.generic_named("User");
    let fields = walk.edges_of(query);
    assert_eq!(
        fields.len(),
        walk.edges_of(user).len(),
        "the mapping copies every field of its source"
    );
    for field in &fields {
        let edge = as_id(&field.args[0]).expect("an edge declares its own id");
        assert!(
            walk.carries("ts.optional", edge),
            "`Partial` made every field optional, `{}` is not",
            as_text(&field.args[2]).unwrap_or_default()
        );
    }

    // The resolve's syntax run answers sites and enumerates nothing, so it owns
    // no shape row of its own and claims no reading of the relation.
    let coverage = walk.coverage();
    assert_eq!(coverage.get(&(0, "tsi.edge".to_string())), None);
    assert_eq!(
        coverage.get(&(walk.semantic_run(), "tsi.edge".to_string())),
        Some(&false),
        "a lib or dependency owner is a leaf, so the relation is not enumerated"
    );
}

/// A callable's inputs are types, and a type that is itself callable carries
/// its own inputs and output: the nesting is what a name-shaped row cannot say.
#[test]
fn a_callable_input_is_itself_a_callable() {
    let walk = Walk::read(PROBE);
    let denoted: BTreeSet<u32> = walk
        .rows_of("tsi.denotes")
        .into_iter()
        .filter_map(|fact| as_id(&fact.args[1]))
        .collect();
    let mut named: Vec<u32> = walk
        .ids_named("map")
        .into_iter()
        .filter(|id| denoted.contains(id))
        .collect();
    assert_eq!(named.len(), 1, "one symbol denotes the method type");
    let map = named.remove(0);
    assert!(walk.carries("tsi.callable", map));

    let project = walk.slot("tsi.input", map, 0);
    assert!(
        walk.carries("tsi.callable", project),
        "the method's one parameter is a function type"
    );
    let element = walk.slot("tsi.input", project, 0);
    let produced = walk.slot("tsi.output", project, 0);
    assert!(walk.ids_named("T").contains(&element));
    assert!(walk.ids_named("U").contains(&produced));
    assert_eq!(
        walk.slot("tsi.output", map, 0),
        produced,
        "the method returns what its projection returns"
    );
}

/// Criterion 5's honest half: a relation the tier states rather than derives
/// stays partial, and the diagnostic beside it says what was left out.
#[test]
fn conformance_is_declared_heritage_and_says_so() {
    let walk = Walk::read(PROBE);
    let user = walk.generic_named("User");
    let mapper = walk.generic_named("Mapper");

    let stated: Vec<&FactOut> = walk
        .rows_of("tsi.conforms")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(user))
        .collect();
    assert_eq!(stated.len(), 1);
    assert_eq!(as_atom(&stated[0].args[2]), Some("declared"));
    let contract = as_id(&stated[0].args[1]).expect("a conformance names its contract");
    let applies: Vec<&FactOut> = walk
        .rows_of("tsi.called")
        .into_iter()
        .filter(|fact| as_id(&fact.args[0]) == Some(contract))
        .collect();
    assert_eq!(applies.len(), 1, "the clause writes `Mapper<T>`");
    assert_eq!(as_id(&applies[0].args[1]), Some(mapper));

    let parameter = walk
        .rows_of("tsi.parameter")
        .into_iter()
        .find(|fact| as_id(&fact.args[1]) == Some(user) && as_int(&fact.args[2]) == Some(0))
        .expect("the class declares one type parameter");
    assert_eq!(as_atom(&parameter.args[3]), Some("unspecified"));
    assert!(walk
        .ids_named("T")
        .contains(&as_id(&parameter.args[0]).expect("a parameter is a type")));

    let semantic = walk.semantic_run();
    assert_eq!(
        walk.coverage().get(&(semantic, "tsi.conforms".to_string())),
        Some(&false)
    );
    assert_eq!(
        walk.diagnostics()
            .get(&(semantic, "tsi.conforms".to_string()))
            .map(String::as_str),
        Some("declared heritage only; structural conformance not enumerated")
    );
}

/// Criterion 5: `complete` means absence from the relation is meaningful, so the
/// set of relations claiming it is the contract a consumer reads.
#[test]
fn the_complete_claims_are_exactly_the_enumerated_relations() {
    let walk = Walk::read(PROBE);
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

/// A type whose own shape names itself terminates only because the second visit
/// reads the id the first minted; bounded expansion would run forever.
#[test]
fn a_recursive_type_closes_through_its_own_id() {
    let walk = Walk::read(RECURSIVE);
    let node = walk.generic_named("Node");
    assert!(walk.carries("tsi.product", node));
    assert!(walk.carries("ts.interface", node));

    let next = walk.edge(node, "next");
    assert_eq!(
        as_id(&next.args[3]),
        Some(node),
        "the edge closes the cycle"
    );
    assert_eq!(as_int(&next.args[4]), Some(1));
    let value = walk.edge(node, "value");
    assert!(walk
        .ids_named("T")
        .contains(&as_id(&value.args[3]).expect("an edge names its target")));
}

/// The reverse door's id closure, run on the producer's own side: an id no row
/// declares would leave a v7 import chasing a node that is not there.
#[test]
fn every_id_is_declared_and_the_stream_survives_the_door() {
    for fixture in [PROBE, RECURSIVE] {
        let walk = Walk::read(fixture);
        let mut declared: BTreeSet<u32> = BTreeSet::new();
        for fact in &walk.facts {
            let position = match fact.relation.as_str() {
                "tsi.type" | "tsi.symbol" | "tsi.value" | "tsi.edge" | "rust.impl" => 0,
                "tsi.called" => 2,
                _ => continue,
            };
            if let Some(id) = as_id(&fact.args[position]) {
                declared.insert(id);
            }
        }
        for fact in &walk.facts {
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

        let scratch = std::env::temp_dir().join("sprefa_a5_ts_semantic");
        std::fs::create_dir_all(&scratch).expect("scratch dir");
        let raw = scratch.join("stream.jsonl");
        std::fs::write(&raw, &walk.stream).expect("write the stream");
        let path = raw.to_str().expect("utf8 path");
        let once = extract(&["--ingest", path]);
        let canonical = scratch.join("once.jsonl");
        std::fs::write(&canonical, &once).expect("write the canonical form");
        let twice = extract(&["--ingest", canonical.to_str().expect("utf8 path")]);
        assert_eq!(once, twice, "{fixture}: the reverse door is idempotent");
    }
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
        DIR,
        "--ts-checker",
        PROBE,
    ]);
    assert!(!stream.is_empty(), "the resolve produced no rows");
    for line in stream.lines() {
        let row: serde_json::Value = serde_json::from_str(line).expect("a row is JSON");
        assert_ne!(row["record"], "fact", "{line}");
        assert_ne!(row["record"], "coverage", "{line}");
        assert!(row.get("fact").is_none(), "{line}");
    }
}
