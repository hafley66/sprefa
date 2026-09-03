//! The rust checker's item walk is priced by the files it was handed, never by
//! the crates those files belong to: the impls it describes are the impls of
//! the types and traits the supplied files declare.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha 141109b17, `run` restored to
//! `Impl::all_in_crate` over every crate a supplied module belongs to, fixture
//! `src/other.rs` in place): measured 2 failed, 1 ignored.
//! `the_walk_describes_impls_of_supplied_types_only` read
//! `{(None, None), (Some("User"), Some("Mapper"))}` over a supplied `lib.rs`
//! alone, the `(None, None)` pair being `other.rs`'s two impls reached through
//! a module no supplied file declares.
//! `every_supplied_impl_is_still_described` read `rust.impl` 3, `tsi.conforms`
//! 3, `tsi.origin` 32, `tsi.type` 32 against the 1, 1, 23, 23 pinned below.
//!
//! Over this crate's own `src/trace.rs` the same sabotage emitted 10 247 rows
//! (`rust.impl` 1139, `tsi.conforms` 1139, `tsi.callable` 998, `tsi.symbol`
//! 999, `tsi.type` 1534) against 203 after (`rust.impl` 7).
//!
//! `walk_time_is_priced_by_the_file` is `#[ignore]`d, so CI load never fails a
//! wall-clock claim; run it with `--ignored`.

#![cfg(feature = "rust-checker")]

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::process::Command;

use sprefa_extract::content_id_of;
use sprefa_extract::tsi::{Arg, FactOut};
use sprefa_extract::FlatFact;

const ROOT: &str = "tests/fixtures/tsi/rust_probe";
const LIB: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";
const OTHER: &str = "tests/fixtures/tsi/rust_probe/src/other.rs";

/// The wall-clock cap the 10-second law puts on the walk. The workspace LOAD
/// carries the SCIP exception (`project.rs` `CHECKER_BUDGET`); the walk does not.
const WALK_CAP_MS: u64 = 10_000;

/// The row cap for one supplied file. `src/trace.rs` is 254 lines and drew
/// 10 247 rows out of the whole-crate walk.
const ITEM_ROW_CAP: usize = 1_000;

/// Every relation the walk enumerates exhaustively over `rust_probe/src/lib.rs`,
/// pinned identically in `102_rust_semantic_tsi.rs`.
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

/// The row count per relation the walk emits for `rust_probe/src/lib.rs`. The
/// file declares its own types and traits, so the by-file filter keeps them all.
const LIB_ROWS: &[(&str, usize)] = &[
    ("rust.assoc", 2),
    ("rust.impl", 1),
    ("rust.lifetime", 1),
    ("rust.ownership", 7),
    ("rust.trait", 1),
    ("tsi.argument", 4),
    ("tsi.callable", 2),
    ("tsi.called", 3),
    ("tsi.conforms", 1),
    ("tsi.denotes", 6),
    ("tsi.edge", 10),
    ("tsi.input", 2),
    ("tsi.origin", 23),
    ("tsi.output", 2),
    ("tsi.parameter", 2),
    ("tsi.primitive", 3),
    ("tsi.product", 4),
    ("tsi.sum", 1),
    ("tsi.symbol", 6),
    ("tsi.type", 23),
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

/// One witnessed, checker-driven resolve over a chosen set of supplied files,
/// with each file's bytes under its own content digest: a span names an id.
struct Walk {
    rows: Vec<FlatFact>,
    facts: Vec<FactOut>,
    blobs: BTreeMap<String, Vec<u8>>,
}

impl Walk {
    fn read(supplied: &[&str]) -> Self {
        let mut args = vec![
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            ROOT,
            "--rust-checker",
        ];
        args.extend_from_slice(supplied);
        let stream = extract(&args);
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
        let blobs = supplied
            .iter()
            .map(|path| {
                let bytes = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(path))
                    .expect("the supplied file is readable");
                (content_id_of(&bytes).to_string(), bytes)
            })
            .collect();
        Self { rows, facts, blobs }
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

    /// The text a span covers, or `None` for a declaration whose file was not
    /// supplied: an external origin carries a path key no blob here answers.
    fn text_at(&self, arg: &Arg) -> Option<String> {
        let Arg::Span(key, start, end) = arg else {
            return None;
        };
        let bytes = self.blobs.get(key)?;
        let slice = bytes.get(*start as usize..*end as usize)?;
        Some(String::from_utf8_lossy(slice).to_string())
    }

    fn written(&self, id: u32) -> Option<String> {
        self.rows_of("tsi.origin")
            .into_iter()
            .find(|fact| matches!(fact.args[0], Arg::Id(named) if named == id))
            .and_then(|fact| self.text_at(&fact.args[2]))
    }

    /// Every `rust.impl` as (owner as written, trait as written). A trait
    /// declared outside the supplied files reads `None`.
    fn implementations(&self) -> BTreeSet<(Option<String>, Option<String>)> {
        self.rows_of("rust.impl")
            .into_iter()
            .map(|fact| {
                let owner = as_id(&fact.args[1]).expect("an impl names its owner");
                let contract = as_id(&fact.args[2]).expect("an impl names its trait");
                (self.written(owner), self.written(contract))
            })
            .collect()
    }

    fn counts(&self) -> BTreeMap<String, usize> {
        let mut counted: BTreeMap<String, usize> = BTreeMap::new();
        for fact in &self.facts {
            *counted.entry(fact.relation.clone()).or_default() += 1;
        }
        counted
    }
}

fn as_id(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Id(id) => Some(*id),
        _ => None,
    }
}

fn owned(pairs: &[(Option<&str>, Option<&str>)]) -> BTreeSet<(Option<String>, Option<String>)> {
    pairs
        .iter()
        .map(|(owner, contract)| (owner.map(str::to_string), contract.map(str::to_string)))
        .collect()
}

/// The value tracing printed for one field of the `rust checker tier loaded`
/// line, which is the only place the walk states its own wall clock.
fn field(line: &str, name: &str) -> Option<u64> {
    let tail = line.split(&format!("{name}=")).nth(1)?;
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The fixture writes `Mapper for User` in `lib.rs` and both `Elsewhere for
/// Other` and `Debug for Other` in `other.rs`, one crate, two modules.
#[test]
fn the_walk_describes_impls_of_supplied_types_only() {
    let from_lib = owned(&[(Some("User"), Some("Mapper"))]);
    let from_other = owned(&[(Some("Other"), Some("Elsewhere")), (Some("Other"), None)]);

    let lib = Walk::read(&[LIB]);
    assert_eq!(
        lib.implementations(),
        from_lib,
        "a supplied `lib.rs` reaches no impl of a type its own module never declared"
    );

    let other = Walk::read(&[OTHER]);
    assert_eq!(other.implementations(), from_other);

    let both = Walk::read(&[LIB, OTHER]);
    assert_eq!(
        both.implementations(),
        from_lib.union(&from_other).cloned().collect(),
        "supplying both files describes both sets"
    );
}

/// The other direction: the filter drops nothing the earlier whole-crate walk
/// described for a file that declares its own types and traits.
#[test]
fn every_supplied_impl_is_still_described() {
    let walk = Walk::read(&[LIB]);
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

    let expected: BTreeMap<String, usize> = LIB_ROWS
        .iter()
        .map(|(relation, count)| ((*relation).to_string(), *count))
        .collect();
    assert_eq!(walk.counts(), expected);

    assert_eq!(
        walk.diagnostics()
            .get(&(semantic, "tsi.conforms".to_string()))
            .map(String::as_str),
        Some("declared impls of supplied types and traits; blanket and auto traits not enumerated")
    );
    assert_eq!(
        walk.diagnostics()
            .get(&(semantic, "tsi.edge".to_string()))
            .map(String::as_str),
        Some("enumerated for owners declared in the supplied files")
    );
}

/// One `--rust-checker` run over this crate's own `src/trace.rs`, with the
/// tier's logged wall clock and the number of rows the item walk produced.
struct Probe {
    walk_ms: u64,
    files: u64,
    rows: usize,
}

fn probe(witness: bool) -> Probe {
    let mut args = vec![
        "--resolve",
        "--family",
        "type",
        "--project-root",
        ".",
        "--rust-checker",
    ];
    if witness {
        args.insert(0, "--witness");
    }
    args.push("src/trace.rs");
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        // A bare `info` turns on rust-analyzer's own span close events, whose
        // formatting cost lands inside the very phase this reads.
        .env("RUST_LOG", "sprefa_extract=info")
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let loaded = stderr
        .lines()
        .find(|line| line.contains("rust checker tier loaded") && line.contains("walk_ms="))
        .unwrap_or_else(|| panic!("the tier logs its own wall clock:\n{stderr}"))
        .to_string();
    let rows = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter(|line| {
            matches!(
                serde_json::from_str::<FlatFact>(line),
                Ok(FlatFact::Fact(_))
            )
        })
        .count();
    Probe {
        walk_ms: field(&loaded, "walk_ms").expect("walk_ms is a number"),
        files: field(&loaded, "files").expect("files is a number"),
        rows,
    }
}

/// The 10-second law over `src/trace.rs`. `walk_ms` also covers the per-file
/// resolve leg, so the item walk's own price is what the envelope adds.
#[test]
#[ignore = "loads a rust-analyzer workspace over this crate twice; run with --ignored"]
fn walk_time_is_priced_by_the_file() {
    let described = probe(true);
    let resolve_only = probe(false);
    assert_eq!(described.files, 1);
    assert_eq!(resolve_only.rows, 0, "no envelope, no item walk");

    let item_walk = described.walk_ms.saturating_sub(resolve_only.walk_ms);
    assert!(
        item_walk < WALK_CAP_MS,
        "item walk {item_walk} ms: witnessed {} ms, resolve-only {} ms",
        described.walk_ms,
        resolve_only.walk_ms
    );
    assert!(
        described.rows < ITEM_ROW_CAP,
        "{} rows for one 254-line file",
        described.rows
    );
}
