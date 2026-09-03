//! The two semantic adapters over an equivalent pair of fixtures: what they
//! agree on rides the shared `tsi.*` relations, what only one language means
//! rides its own namespace, and every row lands in exactly one named bucket.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha 6e5b16a08, measured by deleting
//! every `record=fact` row whose relation starts with `tsi.` from the ts
//! stream before projecting): 4 of the 5 cases fail. The shared set drops from
//! 54 rows to 0, `the_two_streams_share_one_projected_tsi_row_set` prints all
//! 13 SHARED rows as missing, and
//! `every_unshared_tsi_row_is_a_missing_name_or_a_pinned_difference` prints
//! all 10 TS_ASYMMETRIC rows as missing. The 4 `ts.*` rows survive the cut and
//! still fail their own case, reading `ts.interface(_)` and
//! `ts.mapped(_, _, _, _)`: `tsi.origin` is the naming table, so a native row
//! without the shared rows beside it carries no name.

#![cfg(all(feature = "ts-checker", feature = "rust-checker"))]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use sprefa_extract::tsi::{Arg, FactOut, REGISTRY};
use sprefa_extract::FlatFact;

const TS_ROOT: &str = "tests/fixtures/tsi";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_ROOT: &str = "tests/fixtures/tsi/rust_probe";
const RUST_PROBE: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

/// A span carries a content digest and a byte range, so it differs between two
/// fixtures by construction; `tsi.has_type` is a span keyed the same way.
const SPAN_KEYED: &[&str] = &["tsi.origin", "tsi.has_type"];

/// A projected row: the relation, then one word per argument. An `Int` is a
/// position and never a name, so it is written `#0` and stays out of the
/// vocabulary a row is judged against.
type Shape = (&'static str, &'static [&'static str]);

/// The projected `tsi.*` rows both adapters spell over the probe pair. Equality
/// here is criterion 8's receipt: the same shape reached from a `.ts` file
/// through tsc and from a `.rs` file through rust-analyzer.
const SHARED: &[Shape] = &[
    ("tsi.argument", &["_", "#0", "T"]),
    ("tsi.callable", &["map"]),
    ("tsi.called", &["User", "User", "_"]),
    ("tsi.conforms", &["User", "Mapper", "declared"]),
    ("tsi.edge", &["_", "User", "id", "T", "#0"]),
    ("tsi.parameter", &["T", "Mapper", "#0", "unspecified"]),
    ("tsi.parameter", &["T", "User", "#0", "unspecified"]),
    ("tsi.product", &["User"]),
    ("tsi.type", &["Mapper"]),
    ("tsi.type", &["T"]),
    ("tsi.type", &["User"]),
    ("tsi.type", &["_"]),
    ("tsi.type", &["map"]),
];

/// The brief's minimum shared set, with `*` matching one argument. A shape here
/// that leaves SHARED is a lost claim, never a fixture that drifted.
const MINIMUM: &[Shape] = &[
    ("tsi.product", &["User"]),
    ("tsi.type", &["Mapper"]),
    ("tsi.edge", &["_", "User", "id", "T", "#0"]),
    ("tsi.edge", &["_", "User", "name", "*", "#1"]),
    ("tsi.parameter", &["T", "User", "#0", "unspecified"]),
    ("tsi.callable", &["map"]),
    ("tsi.conforms", &["User", "Mapper", "declared"]),
];

/// The ts rows whose every word the rust stream also spells, and which the rust
/// stream still does not carry. Each is an adapter-shape difference, never a
/// missing fixture construct:
///   - a symbol id carries a `tsi.origin` on the ts side and none on the rust
///     side, so every ts `tsi.symbol` and `tsi.denotes` row names its symbol
///     and every rust one reads `_`;
///   - a ts interface is a `tsi.product`, a rust trait is a `tsi.type` with
///     `rust.trait` beside it;
///   - a ts method is an edge of its class and a product of its own, a rust
///     trait method is neither;
///   - `implements Mapper<T>` is an application, `impl Mapper<T> for User<T>`
///     names the trait directly.
const TS_ASYMMETRIC: &[Shape] = &[
    ("tsi.called", &["Mapper", "Mapper", "_"]),
    ("tsi.denotes", &["Mapper", "Mapper"]),
    ("tsi.denotes", &["User", "User"]),
    ("tsi.denotes", &["map", "map"]),
    ("tsi.edge", &["_", "User", "map", "map", "#4"]),
    ("tsi.product", &["Mapper"]),
    ("tsi.product", &["map"]),
    ("tsi.symbol", &["Mapper"]),
    ("tsi.symbol", &["User"]),
    ("tsi.symbol", &["map"]),
];

/// The same list from the rust side. `tsi.input`/`tsi.output` differ because
/// the two `map` signatures differ: ts takes a function type and returns its
/// result, rust takes `T` and returns the impl's associated type.
const RUST_ASYMMETRIC: &[Shape] = &[
    ("tsi.denotes", &["_", "Mapper"]),
    ("tsi.denotes", &["_", "User"]),
    ("tsi.denotes", &["_", "map"]),
    ("tsi.input", &["map", "#0", "T"]),
    ("tsi.output", &["map", "#0", "_"]),
    ("tsi.symbol", &["_"]),
];

const TS_NATIVE: &[Shape] = &[
    ("ts.interface", &["Mapper"]),
    ("ts.mapped", &["Partial", "P", "_", "_"]),
    ("ts.optional", &["_"]),
    ("ts.readonly", &["_"]),
];

const RUST_NATIVE: &[Shape] = &[
    ("rust.assoc", &["Mapper", "Output", "Output"]),
    ("rust.assoc", &["User", "Output", "Vec"]),
    ("rust.impl", &["_", "User", "Mapper"]),
    ("rust.lifetime", &["View", "a"]),
    ("rust.ownership", &["_", "owned"]),
    ("rust.ownership", &["_", "shared"]),
    ("rust.trait", &["Mapper"]),
];

/// A `typescript` the driver can load, the way `tests/101_ts_semantic_tsi.rs`
/// finds one: a checkout's `lib/typescript.js` is the built compiler.
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

/// The reverse door renumbers ids and sorts, so both streams are compared in
/// the one canonical form a consumer would import.
fn canonical(stream: &str, label: &str) -> String {
    let scratch = std::env::temp_dir().join("sprefa_a8_intersection");
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let raw = scratch.join(format!("{label}.jsonl"));
    std::fs::write(&raw, stream).expect("write the stream");
    extract(&["--ingest", raw.to_str().expect("utf8 path")])
}

/// One adapter's canonical stream, projected to shapes.
struct Side {
    shared: BTreeSet<(String, Vec<String>)>,
    native: BTreeSet<(String, Vec<String>)>,
    relations: BTreeSet<String>,
    /// Every word the projection produced, `_` excluded: what this fixture
    /// spells, which is what decides whether the other side COULD match a row.
    vocabulary: BTreeSet<String>,
}

impl Side {
    fn read(stream: String, fixture: &str) -> Self {
        let facts: Vec<FactOut> = stream
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<FlatFact>(line)
                    .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"))
            })
            .filter_map(|row| match row {
                FlatFact::Fact(fact) => Some(fact),
                _ => None,
            })
            .collect();
        let source = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture))
            .expect("the fixture is readable");

        let mut primitive: BTreeMap<u32, String> = BTreeMap::new();
        let mut origin: BTreeMap<u32, String> = BTreeMap::new();
        for fact in &facts {
            match fact.relation.as_str() {
                "tsi.primitive" => {
                    if let (Some(id), Some(class)) = (as_id(&fact.args[0]), as_atom(&fact.args[1]))
                    {
                        primitive.insert(id, class.to_string());
                    }
                }
                "tsi.origin" => {
                    if let (Some(id), Some(text)) =
                        (as_id(&fact.args[0]), text_at(&fact.args[2], &source))
                    {
                        origin.insert(id, text);
                    }
                }
                _ => {}
            }
        }

        let mut side = Self {
            shared: BTreeSet::new(),
            native: BTreeSet::new(),
            relations: BTreeSet::new(),
            vocabulary: BTreeSet::new(),
        };
        for fact in &facts {
            side.relations.insert(fact.relation.clone());
            if SPAN_KEYED.contains(&fact.relation.as_str()) {
                continue;
            }
            let words: Vec<String> = fact
                .args
                .iter()
                .map(|arg| match arg {
                    // A primitive class names the leaf a type system owns; an
                    // origin range names what the fixture wrote; an id with
                    // neither is anonymous and compares only by position.
                    Arg::Id(id) => primitive
                        .get(id)
                        .or_else(|| origin.get(id))
                        .cloned()
                        .unwrap_or_else(|| "_".to_string()),
                    Arg::Text(text) => text.clone(),
                    Arg::Atom(atom) => atom.clone(),
                    Arg::Int(value) => format!("#{value}"),
                    Arg::Span(_, _, _) => "_".to_string(),
                })
                .collect();
            for word in &words {
                if is_name(word) {
                    side.vocabulary.insert(word.clone());
                }
            }
            let row = (fact.relation.clone(), words);
            if fact.relation.starts_with("tsi.") {
                side.shared.insert(row);
            } else {
                side.native.insert(row);
            }
        }
        side
    }
}

/// A word a fixture spells. `_` is an anonymous id and `#n` is a position, so
/// neither says anything about what the other fixture declares.
fn is_name(word: &str) -> bool {
    word != "_" && !word.starts_with('#')
}

fn text_at(arg: &Arg, source: &[u8]) -> Option<String> {
    let Arg::Span(key, start, end) = arg else {
        return None;
    };
    let (start, end) = (*start as usize, *end as usize);
    if end <= start {
        return None;
    }
    if key.starts_with("blake3:") {
        return Some(String::from_utf8_lossy(source.get(start..end)?).to_string());
    }
    // A declaration outside the supplied files keeps its own path where the
    // digest goes, which is how a std or lib type gets named.
    if !Path::new(key).is_absolute() {
        return None;
    }
    let bytes = std::fs::read(key).ok()?;
    Some(String::from_utf8_lossy(bytes.get(start..end)?).to_string())
}

fn as_id(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Id(id) => Some(*id),
        _ => None,
    }
}

fn as_atom(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Atom(atom) => Some(atom),
        _ => None,
    }
}

/// The rust walk is the pair's expensive half, so both streams are read once
/// and every case reads the same two.
fn sides() -> &'static (Side, Side) {
    static SIDES: OnceLock<(Side, Side)> = OnceLock::new();
    SIDES.get_or_init(|| {
        let ts = extract(&[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            TS_ROOT,
            "--ts-checker",
            TS_PROBE,
        ]);
        let rust = extract(&[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            RUST_ROOT,
            "--rust-checker",
            RUST_PROBE,
        ]);
        (
            Side::read(canonical(&ts, "ts"), TS_PROBE),
            Side::read(canonical(&rust, "rust"), RUST_PROBE),
        )
    })
}

fn pinned(rows: &[Shape]) -> BTreeSet<(String, Vec<String>)> {
    rows.iter()
        .map(|(relation, args)| {
            (
                relation.to_string(),
                args.iter().map(|word| word.to_string()).collect(),
            )
        })
        .collect()
}

/// The symmetric difference, one row per line, so a failure reads as a diff.
fn difference(
    left_label: &str,
    left: &BTreeSet<(String, Vec<String>)>,
    right_label: &str,
    right: &BTreeSet<(String, Vec<String>)>,
) -> String {
    let mut report = String::new();
    for (label, rows) in [
        (left_label, left.difference(right)),
        (right_label, right.difference(left)),
    ] {
        for (relation, args) in rows {
            report.push_str(&format!(
                "\n  only in {label}: {relation}({})",
                args.join(", ")
            ));
        }
    }
    report
}

/// Criterion 8's first half: the same projected `tsi.*` rows arrive from a
/// TypeScript file through tsc and from a Rust file through rust-analyzer.
#[test]
fn the_two_streams_share_one_projected_tsi_row_set() {
    let (ts, rust) = sides();
    let observed: BTreeSet<(String, Vec<String>)> =
        ts.shared.intersection(&rust.shared).cloned().collect();
    let expected = pinned(SHARED);
    assert_eq!(
        observed,
        expected,
        "{}",
        difference("the streams", &observed, "SHARED", &expected)
    );
}

/// The claims the brief names, matched with `*` free, so a shape survives a
/// change to the one argument the two languages spell differently.
#[test]
fn every_minimum_claim_is_in_the_shared_set() {
    let (ts, rust) = sides();
    for (relation, pattern) in MINIMUM {
        for (label, side) in [("ts", ts), ("rust", rust)] {
            let matched = side.shared.iter().any(|(name, words)| {
                name == relation
                    && words.len() == pattern.len()
                    && words
                        .iter()
                        .zip(*pattern)
                        .all(|(word, want)| *want == "*" || word == want)
            });
            assert!(
                matched,
                "{label} spells no {relation}({})",
                pattern.join(", ")
            );
        }
    }
}

/// Criterion 8's harder half: a `tsi.*` row one side alone carries is either a
/// name the other fixture never declares, or a pinned adapter difference. No
/// third bucket exists, so a new asymmetry cannot arrive unnoticed.
#[test]
fn every_unshared_tsi_row_is_a_missing_name_or_a_pinned_difference() {
    let (ts, rust) = sides();
    for (label, side, other, expected) in [
        ("ts", ts, rust, TS_ASYMMETRIC),
        ("rust", rust, ts, RUST_ASYMMETRIC),
    ] {
        let observed: BTreeSet<(String, Vec<String>)> = side
            .shared
            .difference(&other.shared)
            .filter(|(_, words)| {
                words
                    .iter()
                    .all(|word| !is_name(word) || other.vocabulary.contains(word))
            })
            .cloned()
            .collect();
        let expected = pinned(expected);
        assert_eq!(
            observed,
            expected,
            "{label}: {}",
            difference("the stream", &observed, "the pinned list", &expected)
        );
    }
}

/// Criterion 8's second half: native meaning stays in its own namespace, and
/// the two namespaces never name the same relation.
#[test]
fn native_rows_are_non_empty_and_the_namespaces_are_disjoint() {
    let (ts, rust) = sides();
    assert_eq!(ts.native, pinned(TS_NATIVE));
    assert_eq!(rust.native, pinned(RUST_NATIVE));
    assert!(!ts.native.is_empty(), "the ts stream carries no native row");
    assert!(
        !rust.native.is_empty(),
        "the rust stream carries no native row"
    );

    let ts_names: BTreeSet<&str> = ts
        .native
        .iter()
        .map(|(relation, _)| relation.as_str())
        .collect();
    let rust_names: BTreeSet<&str> = rust
        .native
        .iter()
        .map(|(relation, _)| relation.as_str())
        .collect();
    let overlap: Vec<&&str> = ts_names.intersection(&rust_names).collect();
    assert!(overlap.is_empty(), "shared native relations: {overlap:?}");
    for name in ts_names {
        assert!(name.starts_with("ts."), "{name} is not a ts relation");
    }
    for name in rust_names {
        assert!(name.starts_with("rust."), "{name} is not a rust relation");
    }
}

/// A relation outside the registry would reach a consumer with no arity and no
/// argument kinds to validate it against.
#[test]
fn every_relation_both_streams_emit_is_in_the_registry() {
    let (ts, rust) = sides();
    let known: BTreeSet<&str> = REGISTRY.iter().map(|row| row.name).collect();
    for relation in ts.relations.union(&rust.relations) {
        assert!(
            known.contains(relation.as_str()),
            "{relation} is in no registry row"
        );
    }
}
