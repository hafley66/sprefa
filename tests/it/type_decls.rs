//! Type-system prototype (2026-07-09): enum brands + named shapes, both
//! typecheck-time only (columns stay text at runtime).
//!   RUNG 1 — `type severity = "error" | "warn" | ...` closed literal set; a
//!            literal outside the set is an `enum-variant-unknown` error with a
//!            nearest-variant suggestion.
//!   RUNG 2 — `type finding(path: text, line: int, sev: severity)` named shape,
//!            referenced by `rel finding_rel: finding.`, expanded at load into a
//!            plain RelDecl so a rule can write rows and a query read them.
//! Same sandbox harness as tests/it/facts.rs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const DL: &str = env!("CARGO_BIN_EXE_dl");

fn sandbox(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("type_decls_{tag}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(dir: &Path, prog: &str) -> (i32, String, String) {
    fs::write(dir.join("p.dl"), prog).unwrap();
    let out = Command::new(DL)
        .arg(dir.join("p.dl"))
        .args(["--db", dir.join("db").to_str().unwrap()])
        .current_dir(dir)
        .output().expect("run dl");
    (out.status.code().unwrap_or(-1),
     String::from_utf8_lossy(&out.stdout).into_owned(),
     String::from_utf8_lossy(&out.stderr).into_owned())
}

/// A valid enum literal in a branded head + query pin passes and returns rows.
#[test]
fn enum_brand_accepts_a_known_variant() {
    let dir = sandbox("enum_ok");
    let prog = concat!(
        "type severity = \"error\" | \"warn\" | \"info\" | \"hint\".\n",
        "rel finding(path: text, line: int, sev: severity).\n",
        "finding(\"a.rs\", 10, \"error\").\n",
        "finding(\"b.rs\", 20, \"warn\").\n",
        "? finding(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "known enum variants must pass:\n{err}");
    assert!(out.contains("(2 rows)"), "both findings selected:\n{out}");
    assert!(out.contains("error") && out.contains("warn"), "{out}");
}

/// An unknown enum literal is an `enum-variant-unknown` error whose message
/// suggests the nearest variant. Exit is non-zero.
#[test]
fn enum_brand_rejects_unknown_variant_with_suggestion() {
    let dir = sandbox("enum_bad");
    let prog = concat!(
        "type severity = \"error\" | \"warn\" | \"info\" | \"hint\".\n",
        "rel finding(path: text, line: int, sev: severity).\n",
        "finding(\"a.rs\", 10, \"wrn\").\n",
        "? finding(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "an unknown enum variant must fail:\n{all}");
    assert!(all.contains("enum-variant-unknown"), "diag code expected:\n{all}");
    assert!(all.contains("did you mean \"warn\"?"), "nearest-variant suggestion expected:\n{all}");
}

/// A named shape expands into a working rel: a rule writes rows, a query reads
/// them. The shape's `sev` column carries the enum brand end to end.
#[test]
fn named_shape_expands_into_a_working_rel() {
    let dir = sandbox("shape_ok");
    let prog = concat!(
        "type severity = \"error\" | \"warn\".\n",
        "type finding(path: text, line: int, sev: severity).\n",
        "rel finding_rel: finding.\n",
        "rel raw(path: text, line: int).\n",
        "raw(\"a.rs\", 10).\n",
        "raw(\"b.rs\", 20).\n",
        // A derived rule fills the shape-declared rel, branding every row \"warn\".
        "finding_rel(path, line, \"warn\") <- raw(path, line).\n",
        "? finding_rel(path, line, sev).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "shape rel must derive rows:\n{err}");
    assert!(out.contains("(2 rows)"), "both raw rows flow into the shape rel:\n{out}");
    assert!(out.contains("a.rs") && out.contains("warn"), "{out}");
}

/// A `rel <name>: <shape>.` naming a shape that was never declared is an
/// `unknown-shape` load error that names the fix.
#[test]
fn unknown_shape_is_a_load_error() {
    let dir = sandbox("shape_unknown");
    let prog = concat!(
        "rel finding_rel: finding.\n",
        "finding_rel(\"a.rs\").\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "an unknown shape must fail:\n{all}");
    assert!(all.contains("unknown-shape"), "diag code expected:\n{all}");
    assert!(all.contains("declare `type finding(...)`"), "message must name the fix:\n{all}");
}

/// Regression: the pre-existing `type X <: Y` nominal brand still works —
/// a valid text value passes, and the brand round-trips through a query.
#[test]
fn plain_nominal_brand_still_works() {
    let dir = sandbox("brand_regress");
    let prog = concat!(
        "type sha <: text.\n",
        "rel commit(id: sha, msg: text).\n",
        "commit(\"abc123\", \"init\").\n",
        "? commit(id, msg).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "nominal brand must still pass:\n{err}");
    assert!(out.contains("abc123") && out.contains("init"), "{out}");
}

// --- Phase 2: enum brands on BUILTIN rels + variant introspection -------------
//
// The agent failure mode: pinning `kind = "fields"` against `type_edge` and
// getting silent zero rows. The builtin kind/action columns now carry ambient
// enum brands (engine::builtin_enum_brands), so a bad pin is a loud
// `enum-variant-unknown` at typecheck; `rel_col` exposes the allowed sets.

/// (a) A typo'd literal pin against `type_edge.kind` is rejected at typecheck
/// with the diag code and a nearest-variant suggestion — no silent zero rows.
#[test]
fn builtin_kind_typo_rejected_with_suggestion() {
    let dir = sandbox("builtin_typo");
    let prog = concat!(
        "rel field_edge(from_type: text, to_type: text).\n",
        "field_edge(from_type, to_type) <- type_edge(from_type, to_type, \"fields\", repo).\n",
        "? field_edge(from_type, to_type).\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "a typo'd builtin kind pin must fail loudly:\n{all}");
    assert!(all.contains("enum-variant-unknown"), "diag code expected:\n{all}");
    assert!(all.contains("did you mean \"field\"?"), "nearest-variant suggestion expected:\n{all}");
    assert!(all.contains("type_edge_kind"), "the brand name orients the fix:\n{all}");
}

/// (b) The correct literal passes typecheck AND joins real extractor rows end
/// to end: a Rust struct field produces a `type_edge(.., "field", ..)` row.
#[test]
fn builtin_kind_correct_literal_accepted() {
    let dir = sandbox("builtin_ok");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/lib.rs"), concat!(
        "struct Id;\n",
        "struct User { id: Id }\n")).unwrap();
    let prog = concat!(
        "rel seen(path: file).\n",
        "seen(path) <- scan(\"WORK\", \"src/**/*.rs\", path, rev).\n",
        "rel field_edge(from_type: text, to_type: text).\n",
        "field_edge(from_type, to_type) <- type_edge(from_type, to_type, \"field\", repo).\n",
        "? field_edge(from_type, to_type).\n");
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "a valid kind pin must pass:\n{err}");
    assert!(out.contains("User") && out.contains("Id"),
        "the field edge User -> Id must surface:\n{out}");
}

/// (c) `rel_col` exposes the variant set: an agent queries the allowed values
/// for type_edge.kind instead of guessing.
#[test]
fn rel_col_exposes_type_edge_kind_variants() {
    let dir = sandbox("introspect");
    let prog = "? rel_col(\"type_edge\", pos, col_name, type, variants).\n";
    let (code, out, err) = run(&dir, prog);
    assert_eq!(code, 0, "rel_col query must run:\n{err}");
    assert!(out.contains("(4 rows)"), "type_edge has 4 columns:\n{out}");
    for kind in ["\"field\"", "\"variant\"", "\"impl\"", "\"generic\"", "\"param\"", "\"returns\"", "\"uses\""] {
        assert!(out.contains(kind), "variants JSON must list {kind}:\n{out}");
    }
}

/// (d) A user `type` decl reusing a builtin enum brand name is a load error
/// naming the conflict (the builtin vocabulary is engine-owned).
#[test]
fn user_decl_colliding_with_builtin_brand_errors() {
    let dir = sandbox("brand_collide");
    let prog = concat!(
        "type type_edge_kind = \"mine\" | \"yours\".\n",
        "rel tagged(name: text, kind: type_edge_kind).\n",
        "tagged(\"a\", \"mine\").\n");
    let (code, out, err) = run(&dir, prog);
    let all = format!("{out}{err}");
    assert_ne!(code, 0, "shadowing a builtin enum brand must fail:\n{all}");
    assert!(all.contains("shadows a built-in enum brand"), "message names the conflict:\n{all}");
}
