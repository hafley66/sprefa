//! An impl whose self type is declared in ANOTHER file keeps its edges.
//!
//! `item_edge_candidates` needed the self type to name an entity of the impl's
//! own file, so `entity_span_named` returned None and the `return` dropped the
//! whole block — its trait leg, its self-type generic arguments and its bounds
//! together. Section 28.3 of `rust.REPORT.md` prices the shape at 294 of the
//! missing `rust.oracle.type.typedecl.tsv` rows (classes E1 and D2).
//!
//! The owner is an `ImplOwner`, never a node: `build_def_index` indexes every
//! named node, so a node here would register the type as declared in each file
//! that impls it and both planes' corpus-unique lookups would go ambiguous.
//!
//! A QUALIFIED self type is excluded. The oracle's `impl_self_name` takes the
//! FIRST path segment, so `impl AsName for tt::Ident` is owned by `tt` (class
//! X3, section 26.3); a row keyed on the trailing segment matches nothing.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, at bb1d46441): the fixture emitted ZERO
//! `resolved_type_edge` rows — all three impls were dropped, and the only other
//! rows the fixture could mint are a `u32` field and a generic parameter.
//!
//! Fixtures: `tests/fixtures/rust_findings/impl_owner/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_findings/impl_owner";

const FILES: &[&str] = &["lib.rs", "decl.rs", "render.rs", "impls.rs"];

fn run() -> Vec<Value> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "call,type".to_string(),
    ];
    args.extend(FILES.iter().map(|name| format!("{DIR}/{name}")));
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
    path.rsplit('/')
        .next()
        .unwrap_or("")
        .trim_end_matches(".rs")
        .to_string()
}

/// (owner file stem, owner_name, target_name, target file stem, kind) per
/// `resolved_type_edge`.
fn type_edges() -> Vec<(String, String, String, String, String)> {
    let mut rows: Vec<(String, String, String, String, String)> = run()
        .iter()
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            (
                stem(&text(row, "owner_path")),
                text(row, "owner_name"),
                text(row, "target_name"),
                stem(&text(row, "target_path")),
                text(row, "kind"),
            )
        })
        .collect();
    rows.sort();
    rows.dedup();
    rows
}

#[test]
fn impl_trait_for_a_type_declared_elsewhere_binds_the_trait() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "impls".to_string(),
            "Widget".to_string(),
            "Render".to_string(),
            "render".to_string(),
            "impl".to_string(),
        )),
        "`impl Render for Widget` binds Render in render.rs: {rows:?}"
    );
}

/// The self type's generic arguments are references even when its head is not
/// an entity of this file — class D2.
#[test]
fn impl_self_type_generic_argument_binds() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "impls".to_string(),
            "Holder".to_string(),
            "Widget".to_string(),
            "decl".to_string(),
            "generic".to_string(),
        )),
        "`impl Holder<Widget>` binds Widget in decl.rs: {rows:?}"
    );
}

#[test]
fn impl_owner_is_owned_by_the_impls_file() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "impls".to_string(),
            "Holder".to_string(),
            "Shade".to_string(),
            "render".to_string(),
            "impl".to_string(),
        )),
        "`impl Shade for Holder<Widget>` is owned by impls.rs: {rows:?}"
    );
}

/// Every type entity ONE file declares, by name.
fn declared_in(file: &str) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_extract"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["--family", "type", &format!("{DIR}/{file}")])
        .output()
        .expect("extract binary runs");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .filter(|row| row["record"] == "node" && text(row, "family") == "type")
        .map(|row| text(&row, "name"))
        .collect()
}

/// The owner is NOT a declaration. If the impl minted a node, `decl.rs` and
/// `impls.rs` would both declare `Widget` and `unique_declared_type`
/// (`rust.rs`) would decline every reference to it corpus-wide.
#[test]
fn an_impl_owner_never_declares_the_type() {
    assert_eq!(declared_in("decl.rs"), vec!["Widget", "Holder"]);
    let impls = declared_in("impls.rs");
    assert!(
        !impls.iter().any(|name| name == "Widget" || name == "Holder"),
        "impls.rs declares neither type it impls: {impls:?}"
    );
}
