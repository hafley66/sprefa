//! Defects the rust-analyzer entrypoint crawl measured
//! (`plans/extract-crawl-2026-08-29/rust.REPORT.md` section 7, kinks 3, 4, 5,
//! 6, 7). Fixtures and their expected/observed headers are in
//! `tests/fixtures/rust_findings/`.
//!
//! FAIL-FIRST RECEIPT, all five red before the fix (binary at c60e5c4cc):
//!   const_block_fns_mint_call_defs
//!     left: {}   right: {"inner", "outer"}
//!   const_block_call_resolves_to_its_sibling
//!     left: []   right: [Edge { caller: "outer", callee: "inner", site: 869,
//!                               kind: "name_resolve" }]
//!   initializer_calls_carry_the_const_or_static_item_as_caller
//!     left: []   right: ["ROW", "TABLE"]
//!   a_closure_caller_edge_mirrors_onto_the_enclosing_fn
//!     left: {"closure@1182"}   right: {"closure@1182", "entry"}
//!   one_mirror_edge_per_closure_caller_edge
//!     no mirror for Edge { caller: "closure@1014", callee: "spawn", site: 1017 }
//!     among 5 rows, 3 of them closure-caller
//!
//! SABOTAGE RECEIPT: deleting the `syn::Item::Const` arm from
//! `call_defs_in_items` restores rows 1 and 2; returning `false` from
//! `initializer_defs` without minting the item def restores row 3; dropping
//! the `enclosing_named_def` push in `Resolve<CallF>` restores rows 4 and 5.
//!
//! FAIL-FIRST RECEIPT for kinks 4 and 7, all four red at b9b98e3af:
//!   a_module_qualified_call_binds_in_the_module_the_path_names
//!     left  [main -> main.rs::main, relative -> util/deep.rs::helper,
//!            relative -> util/deep.rs::local, spread -> main.rs::build,
//!            spread -> main.rs::helper x3]
//!     right [main -> wrapper.rs::main, relative -> util/deep.rs::local,
//!            relative -> util/mod.rs::helper, spread -> main.rs::build,
//!            spread -> util/deep.rs::helper, spread -> util/mod.rs::helper]
//!   a_type_qualifier_keeps_the_name_leg_and_an_unknown_crate_mints_nothing
//!     left: 4   right: 3
//!   a_dropped_site_mints_an_unresolved_row_naming_why
//!     left: []  right: [(ambiguous, first::compute),
//!                       (ambiguous, second::compute), (no_corpus_def, Vec::new)]
//!   one_unresolved_row_per_dropped_site
//!     left: 0   right: 1   (sites 3, edges 2)
//!
//! SABOTAGE RECEIPT for those four: returning `None` from `module_qualifier`
//! restores rows 1 and 2; clearing `drops` on the rust `ResolveArm` restores
//! rows 3 and 4.

use std::collections::BTreeSet;
use std::process::Command;

use serde_json::Value;
use sprefa_extract::{FamilyMask, RustSource, Source};

const CONST_BLOCK_PATH: &str = "tests/fixtures/rust_findings/const_block_defs.rs";
const CONST_BLOCK: &[u8] = include_bytes!("fixtures/rust_findings/const_block_defs.rs");
const UNRESOLVED_PATH: &str = "tests/fixtures/rust_findings/unresolved_reason.rs";
const UNRESOLVED: &[u8] = include_bytes!("fixtures/rust_findings/unresolved_reason.rs");
/// The whole `qualified_path` module tree; every case there needs the others in
/// the resolve universe, so the four paths always travel together.
const QUALIFIED: &[&str] = &[
    "tests/fixtures/rust_findings/qualified_path/main.rs",
    "tests/fixtures/rust_findings/qualified_path/wrapper.rs",
    "tests/fixtures/rust_findings/qualified_path/util/mod.rs",
    "tests/fixtures/rust_findings/qualified_path/util/deep.rs",
];

/// One `resolved_edge` row, reduced to the four fields these cases grade on.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Edge {
    caller: String,
    callee: String,
    site: u64,
    kind: String,
}

/// Every flat fact one `--resolve --family call` run prints.
fn resolve_facts(paths: &[&str]) -> Vec<Value> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(["--family", "call"])
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .collect()
}

fn resolved_edges(paths: &[&str]) -> Vec<Edge> {
    let mut edges: Vec<Edge> = resolve_facts(paths)
        .into_iter()
        .filter(|fact| fact["record"] == "resolved_edge")
        .map(|fact| Edge {
            caller: fact["caller_name"].as_str().unwrap_or("").to_string(),
            callee: fact["callee_name"].as_str().unwrap_or("").to_string(),
            site: fact["caller_site_start"].as_u64().unwrap_or(0),
            kind: fact["kind"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    edges.sort();
    edges
}

/// Kink 5. A `const _: () = { .. }` block is where a derive macro puts whole
/// impl blocks and free fns; the def walker stopped at the item.
#[test]
fn const_block_fns_mint_call_defs() {
    let output = RustSource.extract(CONST_BLOCK_PATH, CONST_BLOCK, FamilyMask::ALL);
    let call = output.call.as_ref().expect("the call family is projected");
    let named: BTreeSet<&str> = call
        .nodes
        .iter()
        .filter_map(|node| node.name.map(|id| output.strings.lookup(id)))
        .collect();
    assert_eq!(named, BTreeSet::from(["inner", "outer"]));
}

/// Kink 5, the edge the defs unblock. `outer` covers the `inner()` site, so the
/// caller binding needs no const-item node here.
#[test]
fn const_block_call_resolves_to_its_sibling() {
    let edges = resolved_edges(&[CONST_BLOCK_PATH]);
    assert_eq!(
        edges,
        vec![Edge {
            caller: "outer".to_string(),
            callee: "inner".to_string(),
            site: 869,
            kind: "name_resolve".to_string(),
        }]
    );
}

/// Kink 6. A call directly in a `static` or `const` initializer sits outside
/// every fn, so the item itself is the caller.
#[test]
fn initializer_calls_carry_the_const_or_static_item_as_caller() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/static_init_call.rs"]);
    let callers: Vec<&str> = edges.iter().map(|edge| edge.caller.as_str()).collect();
    assert_eq!(callers, vec!["ROW", "TABLE"]);
    assert!(
        edges.iter().all(|edge| edge.callee == "helper"),
        "both initializer edges name helper: {edges:?}"
    );
}

/// Kink 6's other half: an item with no call in its initializer stays out of
/// the call plane, so the `const GREETING: &str = "hello"` shape mints no node
/// and the committed wire golden does not move.
#[test]
fn a_const_with_no_call_in_its_initializer_mints_no_call_def() {
    let source = b"pub const GREETING: &str = \"hello\";\nstatic ROOT: u32 = 7;\n";
    let output = RustSource.extract("scratch.rs", source, FamilyMask::ALL);
    let call = output.call.as_ref().expect("the call family is projected");
    assert!(
        call.nodes.is_empty(),
        "a call-free initializer minted {} call def(s)",
        call.nodes.len()
    );
}

/// Kink 3. A closure caller ends every walk, so the edge is ALSO emitted from
/// the innermost enclosing named def and a BFS over named defs passes through.
/// The closure edge stays: it is the only row that says where the call is.
#[test]
fn a_closure_caller_edge_mirrors_onto_the_enclosing_fn() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/closure_caller_chain.rs"]);
    let worker_callers: BTreeSet<&str> = edges
        .iter()
        .filter(|edge| edge.callee == "worker")
        .map(|edge| edge.caller.as_str())
        .collect();
    assert_eq!(worker_callers, BTreeSet::from(["closure@1182", "entry"]));
    assert_eq!(edges.len(), 3, "2 primary edges + 1 mirror: {edges:?}");
}

/// Kink 3's COUNT rail: exactly one mirror per closure-caller edge, never one
/// per closure and never one per named def. Three closures, one nested inside
/// another, so a mirror-per-closure-frame walk would over-count.
#[test]
fn one_mirror_edge_per_closure_caller_edge() {
    let edges = resolved_edges(&["tests/fixtures/rust_findings/closure_mirror_count.rs"]);
    let closure_edges: Vec<&Edge> = edges
        .iter()
        .filter(|edge| edge.caller.starts_with("closure@"))
        .collect();
    let mirrors: Vec<&Edge> = closure_edges
        .iter()
        .map(|closure| {
            edges
                .iter()
                .find(|edge| {
                    edge.site == closure.site
                        && edge.callee == closure.callee
                        && !edge.caller.starts_with("closure@")
                })
                .unwrap_or_else(|| panic!("no mirror for {closure:?} among {edges:?}"))
        })
        .collect();
    assert_eq!(closure_edges.len(), 3, "{edges:?}");
    assert_eq!(mirrors.len(), 3);
    assert!(
        mirrors.iter().all(|mirror| mirror.caller == "entry"),
        "every mirror names the innermost enclosing NAMED def: {mirrors:?}"
    );
    // 5 primaries (one per site) + 3 mirrors. Before the fix: 5.
    assert_eq!(edges.len(), 8, "{edges:?}");
}

/// One resolved edge reduced to the module question: who called, which FILE
/// holds the callee, and what the callee is named there.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Bound {
    caller: String,
    callee_file: String,
    callee: String,
}

/// `(caller, callee file base name, callee)` for every edge, sorted.
fn bound_edges(paths: &[&str]) -> Vec<Bound> {
    let mut bound: Vec<Bound> = resolve_facts(paths)
        .into_iter()
        .filter(|fact| fact["record"] == "resolved_edge")
        .map(|fact| Bound {
            caller: fact["caller_name"].as_str().unwrap_or("").to_string(),
            callee_file: fact["callee_path"]
                .as_str()
                .unwrap_or("")
                .rsplit_once("qualified_path/")
                .map_or_else(|| "?".to_string(), |(_, tail)| tail.to_string()),
            callee: fact["callee_name"].as_str().unwrap_or("").to_string(),
        })
        .collect();
    bound.sort();
    bound
}

/// `(reason, detail)` for every resolve-phase `unresolved` row, sorted.
fn unresolved_rows(paths: &[&str]) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = resolve_facts(paths)
        .into_iter()
        .filter(|fact| fact["record"] == "unresolved")
        .map(|fact| {
            (
                fact["reason"].as_str().unwrap_or("").to_string(),
                fact["detail"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// Kink 4. A site whose `callee_path` names a module binds in THAT module, so
/// `wrapper::main()` stops landing on the caller's own `main`, and two defs of
/// one name are told apart by the path alone.
#[test]
fn a_module_qualified_call_binds_in_the_module_the_path_names() {
    assert_eq!(
        bound_edges(QUALIFIED),
        vec![
            Bound {
                caller: "main".to_string(),
                callee_file: "wrapper.rs".to_string(),
                callee: "main".to_string(),
            },
            Bound {
                caller: "relative".to_string(),
                callee_file: "util/deep.rs".to_string(),
                callee: "local".to_string(),
            },
            Bound {
                caller: "relative".to_string(),
                callee_file: "util/mod.rs".to_string(),
                callee: "helper".to_string(),
            },
            Bound {
                caller: "spread".to_string(),
                callee_file: "main.rs".to_string(),
                callee: "build".to_string(),
            },
            Bound {
                caller: "spread".to_string(),
                callee_file: "util/deep.rs".to_string(),
                callee: "helper".to_string(),
            },
            Bound {
                caller: "spread".to_string(),
                callee_file: "util/mod.rs".to_string(),
                callee: "helper".to_string(),
            },
        ]
    );
}

/// Kink 4's other half: an UPPERCASE qualifier is a type and the module rule
/// must not touch it, and a qualifier naming no corpus module mints nothing
/// rather than falling back to a bare-name guess.
#[test]
fn a_type_qualifier_keeps_the_name_leg_and_an_unknown_crate_mints_nothing() {
    let spread: Vec<Bound> = bound_edges(QUALIFIED)
        .into_iter()
        .filter(|edge| edge.caller == "spread")
        .collect();
    assert_eq!(spread.len(), 3, "3 of the 4 calls in `spread` bind");
    assert!(
        spread.iter().any(|edge| edge.callee == "build"),
        "Widget::build keeps the name leg: {spread:?}"
    );
    assert!(
        !unresolved_rows(QUALIFIED)
            .iter()
            .any(|(_, detail)| detail == "Widget::build"),
        "a bound site mints no unresolved row"
    );
    assert!(
        unresolved_rows(QUALIFIED)
            .contains(&("ambiguous".to_string(), "other_crate::helper".to_string())),
        "other_crate names no corpus module: {:?}",
        unresolved_rows(QUALIFIED)
    );
}

/// Kink 7. Every site the resolve leg drops mints one `unresolved` row, so a
/// caller can tell an external symbol from an ambiguous corpus name. The reason
/// reads the corpus def count for the callee: none at all, or more than one
/// answer this tier does not settle.
#[test]
fn a_dropped_site_mints_an_unresolved_row_naming_why() {
    assert_eq!(
        unresolved_rows(&[UNRESOLVED_PATH]),
        vec![
            ("ambiguous".to_string(), "first::compute".to_string()),
            ("ambiguous".to_string(), "second::compute".to_string()),
            ("no_corpus_def".to_string(), "Vec::new".to_string()),
        ]
    );
}

/// Kink 7's COUNT rail: rows == sites - edges. A per-def or per-name row would
/// pass the shape test above and fail this one.
#[test]
fn one_unresolved_row_per_dropped_site() {
    let output = RustSource.extract(UNRESOLVED_PATH, UNRESOLVED, FamilyMask::ALL);
    let sites = output
        .call
        .as_ref()
        .expect("the call family is projected")
        .aux
        .sites
        .len();
    let edges = resolved_edges(&[UNRESOLVED_PATH]).len();
    let rows = unresolved_rows(&[UNRESOLVED_PATH]).len();
    assert_eq!(sites, 3, "first::compute, second::compute, Vec::new");
    assert_eq!(rows, sites - edges, "sites {sites}, edges {edges}");
}

/// Kink 7 must stay inside the rust arm: the ts arm's phase-1 `unresolved` rows
/// carry no path, and a resolve run over TypeScript gains no row at all.
#[test]
fn the_unresolved_channel_is_the_rust_arm_only() {
    let rows = unresolved_rows(&[
        "tests/fixtures/resolve/0_caller.ts",
        "tests/fixtures/resolve/1_callee.ts",
    ]);
    assert!(rows.is_empty(), "{rows:?}");
}
