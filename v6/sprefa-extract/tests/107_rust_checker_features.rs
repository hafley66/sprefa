//! The checker tier loads every cargo feature, and a supplied file the loaded
//! crate graph declares no module for is named on the semantic run.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha 4eb2fe959, this file plus the two
//! fixture files copied onto it): measured 2 failed, 1 passed. On it
//! `CargoConfig.features` took its default, `Selected { features: [],
//! no_default_features: false }` (ra_ap_project_model cargo_workspace.rs:86),
//! so `a_feature_gated_module_is_walked` found no `Gated` product at all
//! ("left: 0, right: 1"), and `a_file_outside_every_crate_root_is_named` read
//! an empty `tier.*` diagnostic set: a loaded run that had seen nothing was
//! indistinguishable from one that saw everything.
//! `a_walked_file_files_no_diagnostic` is the guard in the other direction and
//! passes on both trees.

#![cfg(feature = "rust-checker")]

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::process::Command;

use sprefa_extract::tsi::{Arg, FactOut, Method};
use sprefa_extract::FlatFact;

const ROOT: &str = "tests/fixtures/tsi/rust_probe";
const GATED: &str = "tests/fixtures/tsi/rust_probe/src/gated.rs";
const ORPHAN: &str = "tests/fixtures/tsi/rust_probe/src/orphan.rs";
const LIB: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

const TIER: &str = "tier.rust-analyzer";

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

/// One witnessed, checker-driven resolve over ONE supplied file, with that
/// file's bytes beside it: an origin span is what names an id.
struct Walk {
    rows: Vec<FlatFact>,
    facts: Vec<FactOut>,
    source: Vec<u8>,
}

impl Walk {
    fn read(supplied: &str) -> Self {
        let stream = extract(&[
            "--witness",
            "--resolve",
            "--family",
            "type",
            "--project-root",
            ROOT,
            "--rust-checker",
            supplied,
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
        let source = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(supplied))
            .expect("the supplied file is readable");
        Self {
            rows,
            facts,
            source,
        }
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

    fn rows_of(&self, wanted: &str) -> Vec<&FactOut> {
        self.facts
            .iter()
            .filter(|fact| fact.relation == wanted)
            .collect()
    }

    /// Every id whose origin span covers exactly `written` inside the supplied
    /// file. A declaration outside it keeps a path key and is skipped.
    fn ids_named(&self, written: &str) -> BTreeSet<u32> {
        self.rows_of("tsi.origin")
            .into_iter()
            .filter(|fact| self.text_at(&fact.args[2]).as_deref() == Some(written))
            .filter_map(|fact| as_id(&fact.args[0]))
            .collect()
    }

    fn text_at(&self, arg: &Arg) -> Option<String> {
        let Arg::Span(key, start, end) = arg else {
            return None;
        };
        if !key.starts_with("blake3:") || end <= start {
            return None;
        }
        let slice = self.source.get(*start as usize..*end as usize)?;
        Some(String::from_utf8_lossy(slice).to_string())
    }

    /// The one fact of `relation` naming `id`, and the run its witness names.
    fn carried_on(&self, relation: &str, id: u32) -> Option<u32> {
        let ordinal = self
            .rows_of(relation)
            .into_iter()
            .find(|fact| as_id(&fact.args[0]) == Some(id))?
            .fact;
        self.rows.iter().find_map(|row| match row {
            FlatFact::Witness(witness)
                if witness.fact == ordinal && witness.method == Method::CheckerWalk =>
            {
                Some(witness.run)
            }
            _ => None,
        })
    }

    /// Every `tier.*` diagnostic: (run, detail). The walk's own partial-coverage
    /// diagnostics name a `tsi.*` relation and are not tier news.
    fn tier_diagnostics(&self) -> Vec<(u32, String, String)> {
        self.rows
            .iter()
            .filter_map(|row| match row {
                FlatFact::Diagnostic(row) if row.relation.starts_with("tier.") => {
                    Some((row.run, row.relation.clone(), row.detail.clone()))
                }
                _ => None,
            })
            .collect()
    }
}

fn as_id(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Id(id) => Some(*id),
        _ => None,
    }
}

/// Row 1: a `#[cfg(feature = "gated")]` module is code, so the tier that loads
/// the crate loads it and the walk reaches every declaration in it.
#[test]
fn a_feature_gated_module_is_walked() {
    let walk = Walk::read(GATED);
    let semantic = walk.semantic_run();

    let mut products: Vec<u32> = walk
        .ids_named("Gated")
        .into_iter()
        .filter(|id| walk.carried_on("tsi.product", *id) == Some(semantic))
        .collect();
    assert_eq!(
        products.len(),
        1,
        "the gated module declares one `Gated` product: {products:?}"
    );
    products.remove(0);

    let callables: Vec<u32> = walk
        .ids_named("make")
        .into_iter()
        .filter(|id| walk.carried_on("tsi.callable", *id) == Some(semantic))
        .collect();
    assert_eq!(
        callables.len(),
        1,
        "the gated module declares one `make` callable: {callables:?}"
    );

    assert!(
        walk.tier_diagnostics().is_empty(),
        "the gated file owns a module: {:?}",
        walk.tier_diagnostics()
    );
}

/// Row 2: the tier LOADED and still saw nothing in the file it was handed, so
/// the stream says which file, on the run that saw nothing.
#[test]
fn a_file_outside_every_crate_root_is_named() {
    let walk = Walk::read(ORPHAN);
    let semantic = walk.semantic_run();
    let filed = walk.tier_diagnostics();
    assert_eq!(filed.len(), 1, "one unmodulated file, one row: {filed:?}");
    assert_eq!(
        filed[0].0, semantic,
        "the tier loaded, so it is not a decline"
    );
    assert_eq!(filed[0].1, TIER);
    assert!(
        filed[0].2.contains("orphan.rs"),
        "the row names the file: {}",
        filed[0].2
    );
    assert!(
        walk.facts.is_empty(),
        "a file owning no module declares nothing: {} rows",
        walk.facts.len()
    );
}

/// The other direction: a file the crate graph does declare a module for files
/// no tier row at all.
#[test]
fn a_walked_file_files_no_diagnostic() {
    let walk = Walk::read(LIB);
    let filed = walk.tier_diagnostics();
    assert!(
        filed.is_empty(),
        "a walked file is not tier news: {filed:?}"
    );
    assert!(!walk.facts.is_empty(), "the walk reached the fixture");
}
