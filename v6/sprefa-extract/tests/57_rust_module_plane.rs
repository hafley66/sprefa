//! The rust module plane: the Rust Reference's own `use`/`mod` resolution,
//! run once per file set, so an imported name binds the way the compiler
//! binds it and name-matching across files is only what a FREE name falls
//! to. Mirrors `54_ts_module_plane.rs`.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): before the plane,
//! `IndexBag.rust_modules` did not exist and `Resolve<CallF>`/`Resolve<TypeF>`
//! had no import leg, so `crate_path_caller` in the fixture below resolved
//! `crate_path_fn()` (unqualified) against the whole corpus name-match,
//! `super_caller`'s `root_target()` likewise, and neither `resolved_import`
//! row nor the `ambiguous`/`conflict` drop for the glob collision existed.
//!
//! Fixtures: `tests/fixtures/rust_findings/module_plane/`.

use std::path::Path;
use std::process::Command;
use std::time::Instant;

use serde_json::Value;

const CRATE_A: &str = "tests/fixtures/rust_findings/module_plane/crate_a/src";
const CRATE_B: &str = "tests/fixtures/rust_findings/module_plane/crate_b/src";

const FIXTURE_FILES: &[&str] = &[
    "lib.rs",
    "nested.rs",
    "real_target.rs",
    "hop_source.rs",
    "hop_one.rs",
    "hop_two.rs",
    "glob_one.rs",
    "glob_two.rs",
    "renamed_src.rs",
];

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call".to_string(),
    ];
    args.extend(names.iter().map(|name| format!("{CRATE_A}/{name}")));
    args.push(format!("{CRATE_B}/lib.rs"));
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(&args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a flat fact is JSON"))
        .collect()
}

fn text(row: &Value, key: &str) -> String {
    row[key].as_str().unwrap_or("").to_string()
}

fn stem(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).trim_end_matches(".rs").to_string()
}

/// `(caller, callee file stem, callee, kind)` per `resolved_edge`, sorted.
fn edges(names: &[&str]) -> Vec<(String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                stem(&text(row, "callee_path")),
                text(row, "callee_name"),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `(src stem, local, name, target stem, target_name, kind, hops)` per
/// `resolved_import`.
fn imports(names: &[&str]) -> Vec<(String, String, String, String, String, String, u64)> {
    let mut rows: Vec<(String, String, String, String, String, String, u64)> = run(names)
        .iter()
        .filter(|row| row["record"] == "resolved_import")
        .map(|row| {
            (
                stem(&text(row, "src_path")),
                text(row, "local"),
                text(row, "name"),
                stem(&text(row, "target_path")),
                text(row, "target_name"),
                text(row, "kind"),
                row["hops"].as_u64().unwrap_or(0),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `(path stem, reason, detail)` per `unresolved`.
fn unresolved(names: &[&str]) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "unresolved")
        .map(|row| (stem(&text(row, "path")), text(row, "reason"), text(row, "detail")))
        .collect();
    rows.sort();
    rows
}

/// A `use crate::nested::crate_path_fn;` (`crate::`) call binds to `nested.rs`,
/// never a name-match guess.
#[test]
fn a_crate_qualified_use_binds_through_the_plane() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "crate_path_caller".to_string(),
        "nested".to_string(),
        "crate_path_fn".to_string(),
        "import_resolve".to_string(),
    )));
}

/// `use super::root_target;` inside `nested.rs` binds to the crate root file.
#[test]
fn a_super_qualified_use_binds_through_the_plane() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "super_caller".to_string(),
        "lib".to_string(),
        "root_target".to_string(),
        "import_resolve".to_string(),
    )));
}

/// `#[path = "real_target.rs"] mod path_mod;` : the `use` names `path_mod`,
/// the file on disk is `real_target.rs`, and the plane binds through the
/// override rather than the file's own path-derived module name.
#[test]
fn a_path_attribute_module_binds_to_its_override() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "path_mod_caller".to_string(),
        "real_target".to_string(),
        "path_fn".to_string(),
        "import_resolve".to_string(),
    )));
}

/// One `pub use` hop: `hop_one.rs` re-exports `hop_source::hop_target_fn`.
/// hop_one's OWN binding is `local`, hops=0: it names hop_source's LOCAL
/// declaration directly. `two_pub_use_hops_resolve_with_hops_two` below is
/// where a THIRD file goes through hop_one and sees `indirect`, hops=1.
#[test]
fn one_pub_use_hop_resolves_locally_with_hops_zero() {
    assert!(imports(FIXTURE_FILES).contains(&(
        "hop_one".to_string(),
        "reexported_fn".to_string(),
        "hop_target_fn".to_string(),
        "hop_source".to_string(),
        "hop_target_fn".to_string(),
        "local".to_string(),
        0,
    )));
}

/// A second `pub use` hop over the first: `lib.rs` imports through
/// `hop_two.rs`, which itself imports through `hop_one.rs`. hops counts BOTH.
#[test]
fn two_pub_use_hops_resolve_with_hops_two() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "hop_two_caller".to_string(),
        "hop_source".to_string(),
        "hop_target_fn".to_string(),
        "import_resolve".to_string(),
    )));
    assert!(imports(FIXTURE_FILES).contains(&(
        "lib".to_string(),
        "reexported_fn_two".to_string(),
        "reexported_fn_two".to_string(),
        "hop_source".to_string(),
        "hop_target_fn".to_string(),
        "indirect".to_string(),
        2,
    )));
}

/// `use crate::renamed_src::original_name as renamed_local;` binds the ALIAS
/// to the source declaration; `name` on the row stays the asked name.
#[test]
fn a_renamed_use_binds_by_source_name_under_the_local_alias() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "renamed_caller".to_string(),
        "renamed_src".to_string(),
        "original_name".to_string(),
        "import_resolve".to_string(),
    )));
    assert!(imports(FIXTURE_FILES).contains(&(
        "lib".to_string(),
        "renamed_local".to_string(),
        "original_name".to_string(),
        "renamed_src".to_string(),
        "original_name".to_string(),
        "local".to_string(),
        0,
    )));
}

/// `use crate::glob_one::*; use crate::glob_two::*;` bring `glob_a_fn` into
/// scope, but lib.rs ALSO declares its own private `fn glob_a_fn`: the local
/// item wins, no ambiguity (Rust's own shadowing rule).
#[test]
fn a_local_item_shadows_a_glob_brought_name() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "shadow_caller".to_string(),
        "lib".to_string(),
        "glob_a_fn".to_string(),
        "name_resolve".to_string(),
    )));
}

/// `conflict` is defined in BOTH glob sources with no local shadow: the two
/// star arms disagree, so the site drops with reason `ambiguous`.
#[test]
fn two_globs_offering_the_same_name_drop_ambiguous() {
    assert!(!edges(FIXTURE_FILES).iter().any(|(caller, ..)| caller == "glob_ambiguous_caller"));
    assert!(unresolved(FIXTURE_FILES).contains(&(
        "lib".to_string(),
        "ambiguous".to_string(),
        "conflict".to_string(),
    )));
}

/// `mod inline_holder { pub fn inline_fn() {} }` then `use
/// crate::inline_holder::inline_fn;`: the def lives in the SAME blob as the
/// call site, so the existing same-file leg binds it (kind stays
/// `name_resolve`); the plane still resolves the `use` for the import row.
#[test]
fn an_inline_module_resolves_through_the_same_blob() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "inline_caller".to_string(),
        "lib".to_string(),
        "inline_fn".to_string(),
        "name_resolve".to_string(),
    )));
    assert!(imports(FIXTURE_FILES).contains(&(
        "lib".to_string(),
        "inline_fn".to_string(),
        "inline_fn".to_string(),
        "lib".to_string(),
        "inline_fn".to_string(),
        "local".to_string(),
        0,
    )));
}

/// `use crate_b::cross_fn;` crosses a crate boundary the corpus DOES carry:
/// the plane has no crate manifest reader, so this is suffix-matched like any
/// other absolute qualifier, and it is the shape a workspace needs to work.
#[test]
fn a_cross_crate_use_binds_when_the_target_crate_is_in_the_corpus() {
    assert!(edges(FIXTURE_FILES).contains(&(
        "cross_crate_caller".to_string(),
        "lib".to_string(),
        "cross_fn".to_string(),
        "import_resolve".to_string(),
    )));
}

/// `use std::collections::HashMap;`: no corpus file spells `std`, so the
/// binding has no row at all (not `external`, the closed vocabulary stays at
/// `no_corpus_def`/`ambiguous`, both corpus-wide facts already carried).
#[test]
fn an_external_std_use_mints_no_import_row() {
    assert!(!imports(FIXTURE_FILES).iter().any(|(_, local, ..)| local == "StdMapUnused"));
}

/// One `resolved_import` row per RESOLVED `use` leaf in the fixture: an
/// ambiguous/external one has none, matching `bindings()`'s own contract.
#[test]
fn edge_count_matches_the_fixtures_written_bindings() {
    let count = imports(FIXTURE_FILES).len();
    assert_eq!(count, 9, "imports: {:?}", imports(FIXTURE_FILES));
}

// ── the plane's cost ────────────────────────────────────────────────────────

const RATIO_BUDGET: f64 = 2.5;

/// A barrel corpus of `n` leaf modules, one barrel re-exporting all of them
/// via `pub use ..::*;`, and `n` consumers each importing one name through
/// it: the shape that makes the plane work hardest, one star walk per name.
fn barrel_corpus(dir: &Path, n: usize) -> Vec<String> {
    let dir = dir.join(format!("n{n}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    let mut paths = Vec::new();
    let mut barrel = String::new();
    for index in 0..n {
        let leaf = dir.join(format!("leaf{index}.rs"));
        std::fs::write(&leaf, format!("pub fn pick{index}(n: u32) -> u32 {{ n + {index} }}\n"))
            .expect("leaf file");
        paths.push(leaf.to_string_lossy().into_owned());
        barrel.push_str(&format!("pub use crate::leaf{index}::*;\n"));
    }
    let barrel_path = dir.join("barrel.rs");
    std::fs::write(&barrel_path, barrel).expect("barrel file");
    paths.push(barrel_path.to_string_lossy().into_owned());
    for index in 0..n {
        let consumer = dir.join(format!("use{index}.rs"));
        std::fs::write(
            &consumer,
            format!(
                "use crate::barrel::pick{index};\n\npub fn call{index}() -> u32 {{ pick{index}({index}) }}\n"
            ),
        )
        .expect("consumer file");
        paths.push(consumer.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(args: &[String]) -> f64 {
    let start = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .arg("--family")
        .arg("call")
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    start.elapsed().as_secs_f64()
}

/// COUNT test on cost: doubling the corpus must not more than 2.5x the wall.
/// A resolver that re-walked a barrel's star list per call site instead of
/// per binding would show up here as a quadratic, not as a wrong answer.
#[test]
fn barrel_resolve_wall_grows_linearly_with_file_count() {
    let dir = std::env::temp_dir().join("sprefa-extract-57-rust-module-plane");
    std::fs::create_dir_all(&dir).expect("scratch root");
    let small = barrel_corpus(&dir, 200);
    let large = barrel_corpus(&dir, 400);
    let wall200 = resolve_wall(&small);
    let wall400 = resolve_wall(&large);
    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
