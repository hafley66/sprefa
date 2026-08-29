//! `Resolve<TypeF>` for go binds a bare type name to a `type` DECLARATION, not
//! to whatever entity in the name index happens to share the name. A go method
//! and a type live in one namespace as far as `DefIndex` is concerned, so
//! `ModifierFlags ModifierFlags` inside a struct used to bind to the method
//! `func (n *Node) ModifierFlags()` in the referring file.
//!
//! Fail-first on f5fd6dbf9, before `src/lang/go.rs`'s type-decl filter:
//!
//! ```text
//! ---- a_field_named_like_its_type_binds_the_type_declaration stdout ----
//! thread 'a_field_named_like_its_type_binds_the_type_declaration' panicked at
//! tests/68_go_type_refs.rs:74:5:
//! ModifierList -> ModifierFlags must target a.go, rows: [
//!   ("ModifierList", "b.go", "ModifierFlags", "field"),
//!   ("Wrapper", "b.go", "ModifierList", "field")]
//! ```
//!
//! Fixture: `tests/fixtures/go_type_refs`, the `internal/ast/ast.go` +
//! `internal/ast/modifierflags.go` shape of typescript-go reduced to two files.

use std::process::Command;

use serde_json::Value;

const FILES: &[&str] = &["a.go", "b.go", "other/c.go"];

fn type_edges() -> Vec<(String, String, String, String)> {
    let mut args: Vec<String> = vec![
        "--resolve".to_string(),
        "--family".to_string(),
        "type".to_string(),
    ];
    args.extend(
        FILES
            .iter()
            .map(|name| format!("tests/fixtures/go_type_refs/{name}")),
    );
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
    let mut rows: Vec<(String, String, String, String)> = String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).expect("a flat fact is JSON"))
        .filter(|row| row["record"] == "resolved_type_edge")
        .map(|row| {
            let text = |key: &str| row[key].as_str().unwrap_or("").to_string();
            let path = text("target_path");
            (
                text("owner_name"),
                path.rsplit('/').next().unwrap_or(&path).to_string(),
                text("target_name"),
                text("kind"),
            )
        })
        .collect();
    rows.sort();
    rows
}

/// `type ModifierList struct { ModifierFlags ModifierFlags }` in `b.go`, with
/// `type ModifierFlags uint32` in `a.go` and a method `ModifierFlags` in
/// `b.go`: only the type declaration can be the target.
#[test]
fn a_field_named_like_its_type_binds_the_type_declaration() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "ModifierList".to_string(),
            "a.go".to_string(),
            "ModifierFlags".to_string(),
            "field".to_string(),
        )),
        "ModifierList -> ModifierFlags must target a.go, rows: {rows:?}"
    );
}

/// `type Snapshot` is declared in BOTH packages, so the corpus-wide name match
/// abstains and only go's package scope can bind `Session.Snapshot` to `a.go`.
/// Fail-first on f5fd6dbf9: no `Session` row at all.
#[test]
fn a_bare_type_ref_binds_inside_its_own_package() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "Session".to_string(),
            "a.go".to_string(),
            "Snapshot".to_string(),
            "field".to_string(),
        )),
        "Session -> Snapshot must target a.go, rows: {rows:?}"
    );
}

/// The filter narrows to type declarations, it does not narrow to OTHER files:
/// `Wrapper`'s `ModifierList` field still binds same-file.
#[test]
fn a_same_file_type_ref_still_binds_same_file() {
    let rows = type_edges();
    assert!(
        rows.contains(&(
            "Wrapper".to_string(),
            "b.go".to_string(),
            "ModifierList".to_string(),
            "field".to_string(),
        )),
        "Wrapper -> ModifierList must target b.go, rows: {rows:?}"
    );
}
