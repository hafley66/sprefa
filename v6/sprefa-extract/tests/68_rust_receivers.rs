//! The rust receiver leg: a method site `x.m()` binds through `x`'s declared
//! type T when a corpus impl block (inherent or trait) defines `m` for T, and
//! `T::f()`/`Self::f()` bind through the same (T, m) table. An unknown
//! receiver stays `inferred`; a two-glob collision stays unresolved.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, whole file): before this leg the rust arm
//! had no `CallFAux.receivers` rows and no impl table, so `param_typed`'s
//! `w.build()` and every `Widget::new()`/`Self::new()` site fell to the
//! corpus-wide name match and dropped `ambiguous`.
//!
//! Fixtures: `tests/fixtures/rust_findings/receivers/src/`.

use std::process::Command;

use serde_json::Value;

const SRC: &str = "tests/fixtures/rust_findings/receivers/src";

fn run(names: &[&str]) -> Vec<Value> {
    let mut args: Vec<String> = vec!["--resolve".to_string(), "--family".to_string(), "call".to_string()];
    args.extend(names.iter().map(|name| format!("{SRC}/{name}")));
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

/// (caller_name, callee_name, callee file stem) per `resolved_edge`.
fn edges(names: &[&str]) -> Vec<(String, String, String)> {
    let mut rows: Vec<(String, String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "resolved_edge")
        .map(|row| {
            (
                text(row, "caller_name"),
                text(row, "callee_name"),
                text(row, "callee_path")
                    .rsplit('/')
                    .next()
                    .unwrap_or("")
                    .trim_end_matches(".rs")
                    .to_string(),
            )
        })
        .collect();
    rows.sort();
    rows
}

fn drops(names: &[&str]) -> Vec<(String, String)> {
    let mut rows: Vec<(String, String)> = run(names)
        .iter()
        .filter(|row| row["record"] == "unresolved" && row["family"] == "call")
        .map(|row| (text(row, "detail"), text(row, "reason")))
        .collect();
    rows.sort();
    rows
}

fn has(rows: &[(String, String, String)], caller: &str, callee: &str, file: &str) -> bool {
    rows.iter().any(|(c, n, f)| c == caller && n == callee && f == file)
}

#[test]
fn param_typed_receiver_binds_inherent_method() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "param_typed", "build", "lib"), "{rows:?}");
}

#[test]
fn let_typed_receiver_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "let_typed", "build", "lib"), "{rows:?}");
}

#[test]
fn one_hop_through_result_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "one_hop", "build", "lib"), "{rows:?}");
}

#[test]
fn field_receiver_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "uses_field", "build", "lib"), "{rows:?}");
}

#[test]
fn trait_impl_method_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "hello", "build", "lib"), "{rows:?}");
}

#[test]
fn self_assoc_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "from_self", "new", "lib"), "{rows:?}");
}

#[test]
fn type_assoc_binds() {
    let rows = edges(&["lib.rs"]);
    assert!(has(&rows, "let_typed", "new", "lib"), "{rows:?}");
    assert!(has(&rows, "make_widget", "new", "lib"), "{rows:?}");
}

#[test]
fn unknown_receiver_drops_inferred() {
    // `into`'s receiver is a method-chain tail the walk cannot type: the
    // site drops with the `inferred` reason, never `ambiguous`.
    let rows = drops(&["lib.rs"]);
    assert!(
        rows.iter().any(|(detail, reason)| detail == "into" && reason == "inferred"),
        "{rows:?}"
    );
}

/// `recv_alpha.rs` and `recv_beta.rs` both define `tick`, so the corpus name
/// match declines every site in `recv_sites.rs` and ONLY the receiver leg can
/// bind one. `recv_sites.rs` defines no `tick` of its own, so the same-file
/// leg cannot rescue it either.
const MULTI: [&str; 3] = ["recv_sites.rs", "recv_alpha.rs", "recv_beta.rs"];

/// SABOTAGE RECEIPT: `visit_local`'s `_ => return` arm returned before
/// `syn::visit::visit_local`, so the initializer of a destructuring `let` was
/// never walked and `a.tick()` inside one got NO receiver row:
/// `assertion failed: has(&rows, "tuple_pattern_init", "tick", "recv_alpha")`.
#[test]
fn tuple_pattern_initializer_binds() {
    let rows = edges(&MULTI);
    assert!(has(&rows, "tuple_pattern_init", "tick", "recv_alpha"), "{rows:?}");
}

/// SABOTAGE RECEIPT: the same hole through a `let ... else`, the shape 2,232
/// of the corpus's 15,444 `ambiguous` method sites carry:
/// `assertion failed: has(&rows, "let_else_init", "tick", "recv_alpha")`.
#[test]
fn let_else_initializer_binds() {
    let rows = edges(&MULTI);
    assert!(has(&rows, "let_else_init", "tick", "recv_alpha"), "{rows:?}");
}

/// SABOTAGE RECEIPT: `let a = a.tick()` inserted the new `a` before the
/// initializer was walked, so the receiver read its own binding as unknown:
/// `assertion failed: has(&rows, "shadowed_by_own_init", "tick", "recv_alpha")`.
#[test]
fn initializer_reads_the_outer_binding() {
    let rows = edges(&MULTI);
    assert!(has(&rows, "shadowed_by_own_init", "tick", "recv_alpha"), "{rows:?}");
}

/// SABOTAGE RECEIPT: `fn new() -> Self` put the literal string `Self` in the
/// one-hop return table, so `let b = Beta::new(); b.tick()` asked the impl
/// table for `("Self", "tick")`, found nothing, and — the receiver type being
/// KNOWN — emitted no row at all:
/// `assertion failed: has(&rows, "beta_one_hop", "tick", "recv_beta")`.
#[test]
fn self_return_one_hop_binds() {
    let rows = edges(&MULTI);
    assert!(has(&rows, "beta_one_hop", "tick", "recv_beta"), "{rows:?}");
}

#[test]
fn single_glob_source_binds() {
    let rows = edges(&["glob_single.rs", "glob_src.rs"]);
    assert!(has(&rows, "caller", "glob_target", "glob_src"), "{rows:?}");
}

#[test]
fn two_glob_sources_stay_unresolved() {
    let rows = drops(&["glob_two.rs", "glob_a.rs", "glob_b.rs"]);
    assert!(
        rows.iter().any(|(detail, reason)| detail == "shadowed" && reason == "ambiguous"),
        "{rows:?}"
    );
}
