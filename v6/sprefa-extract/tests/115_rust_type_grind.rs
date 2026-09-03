//! The rust type grind (RATCHET rows `rust.type.syntax.oracle-typedecl` and
//! `rust.type.checker.oracle-typedecl`), two mechanisms.
//!
//! 1. An impl's self-type HEAD is a reference. The typedecl oracle walks every
//!    path under an impl and keys the block on its self type, so `impl Foo`
//!    and `impl Bar for Foo` each carry `Foo -> Foo`. Classes X2 (same file)
//!    and X2b (declared elsewhere) of `rust.REPORT.md` sec 28.2, 1,378 + 302
//!    oracle rows at 1b2464c9b, excluded until now by an agent's warrant, never
//!    a ruling. Bare heads only: a qualified head is owned by its qualifier
//!    (class X3) and matches nothing.
//!
//!    FAIL-PRE-FIX RECEIPT (at 1b2464c9b, `rust.type_census.py` over the
//!    checker dump): `X2 self-edge, same file 1378`, `X2b self-edge,
//!    cross-file 302`, both classes empty of `ours` rows. On this fixture the
//!    only rows were `u32` fields (no edge) and `Widget -> Render`.
//!
//! 2. The checker's type answers were keyed on (file, name), so a name one file
//!    resolves two ways bound nothing (`type_ambiguous`, REPORT sec 33.3). The
//!    per-file answers are now a start-sorted list; a collision narrows to the
//!    references spelled as the candidate is, then to the one nearest the
//!    owner. `rust_checker_types` `Mixed` is the receipt: `94_rust_checker_types.rs`
//!    pinned both spellings falling back to `module_plane`; they now carry
//!    origin `checker`, one per declaration.
//!
//! Fixtures: `tests/fixtures/rust_type_grind/`, `tests/fixtures/rust_checker_types/`.

use std::process::Command;

use serde_json::Value;

const DIR: &str = "tests/fixtures/rust_type_grind";

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

fn row(owner_file: &str, owner: &str, target: &str, target_file: &str, kind: &str) -> (String, String, String, String, String) {
    (
        owner_file.to_string(),
        owner.to_string(),
        target.to_string(),
        target_file.to_string(),
        kind.to_string(),
    )
}

/// Class X2: `impl Local` in the file declaring `Local`.
#[test]
fn an_impl_self_type_in_its_declaring_file_references_the_declaration() {
    let rows = type_edges();
    assert!(
        rows.contains(&row("decl", "Local", "Local", "decl", "uses")),
        "`impl Local` carries `Local -> Local` in decl.rs: {rows:?}"
    );
}

/// Class X2b: `impl Widget` and `impl Render for Widget` in a file that does
/// not declare `Widget`; the `ImplOwner` is the src, decl.rs the dst.
#[test]
fn an_impl_self_type_declared_elsewhere_references_that_file() {
    let rows = type_edges();
    assert!(
        rows.contains(&row("impls", "Widget", "Widget", "decl", "uses")),
        "`impl Widget` in impls.rs binds decl.rs's Widget: {rows:?}"
    );
    assert!(
        rows.contains(&row("impls", "Widget", "Render", "render", "impl")),
        "the trait leg of the same block stays: {rows:?}"
    );
}

/// Class X3 stays out: the oracle owns `impl crate::decl::Local` by `crate`.
#[test]
fn a_qualified_self_type_head_mints_no_row() {
    let rows = type_edges();
    assert!(
        !rows
            .iter()
            .any(|(owner_file, owner, _, _, _)| owner_file == "impls" && owner == "Local"),
        "a qualified head has no owner in impls.rs: {rows:?}"
    );
}

#[cfg(feature = "rust-checker")]
mod checker {
    use std::process::Command;

    use serde_json::Value;

    const DIR: &str = "tests/fixtures/rust_checker_types";
    const FILES: &[&str] = &["src/lib.rs", "src/widget.rs", "src/decoys.rs"];

    /// (owner_name, target file tail, target_name, resolution origin).
    fn checker_edges() -> Vec<(String, String, String, String)> {
        let mut args: Vec<String> = vec![
            "--resolve".to_string(),
            "--family".to_string(),
            "call,type".to_string(),
            "--project-root".to_string(),
            DIR.to_string(),
            "--rust-checker".to_string(),
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
        let mut rows: Vec<(String, String, String, String)> = String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_str::<Value>(line).expect("one json fact per line"))
            .filter(|fact| fact["record"] == "resolved_type_edge")
            .map(|fact| {
                (
                    fact["owner_name"].as_str().unwrap_or_default().to_string(),
                    fact["target_path"]
                        .as_str()
                        .unwrap_or_default()
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_string(),
                    fact["target_name"].as_str().unwrap_or_default().to_string(),
                    fact["resolution_origin"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                )
            })
            .collect();
        rows.sort();
        rows.dedup();
        rows
    }

    /// `Mixed` spells `Config` bare (the glob binding, widget.rs) and as
    /// `decoys::Config`; each candidate takes the answer spelled its way.
    #[test]
    fn a_name_one_file_resolves_two_ways_answers_each_spelling() {
        let edges = checker_edges();
        for file in ["widget.rs", "decoys.rs"] {
            assert!(
                edges.contains(&(
                    "Mixed".to_string(),
                    file.to_string(),
                    "Config".to_string(),
                    "checker".to_string(),
                )),
                "Mixed -> {file} Config carries origin checker, got {edges:?}"
            );
        }
    }
}
