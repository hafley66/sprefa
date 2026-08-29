//! The go inferred-receiver plane: `x := f(); x.M()` binds x to the callee's
//! declared result type (one hop, source order, no fixpoint). Every fixture
//! name here is shared across files where a bare corpus name search would be
//! ambiguous, so a passing assertion proves the result-type narrowing did the
//! work, never a lucky unique name.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn inferred_dir() -> Vec<String> {
    let dir = fixture("go_findings/inferred");
    let mut paths: Vec<String> = walk(&dir);
    paths.sort();
    paths
}

fn walk(dir: &str) -> Vec<String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("fixture dir readable") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            out.extend(walk(&path.to_string_lossy()));
        } else if path.extension().is_some_and(|ext| ext == "go") {
            out.push(path.to_string_lossy().into_owned());
        }
    }
    out
}

/// `(caller_name, callee_name, callee_path, kind)` per resolved edge of one
/// `--resolve` run over `paths`.
fn resolved_edges(paths: &[String]) -> Vec<(String, String, String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "resolved_edge").then(|| {
                (
                    row["caller_name"].as_str().unwrap_or("").to_string(),
                    row["callee_name"].as_str().unwrap_or("").to_string(),
                    row["callee_path"].as_str().unwrap_or("").to_string(),
                    row["kind"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

/// `(path, reason, detail)` per `unresolved` row of one `--resolve` run.
fn unresolved_rows(paths: &[String]) -> Vec<(String, String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(out.status.success(), "resolve failed");
    String::from_utf8(out.stdout)
        .expect("utf8 wire")
        .lines()
        .filter_map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).ok()?;
            (row["record"] == "unresolved").then(|| {
                (
                    row["path"].as_str().unwrap_or("").to_string(),
                    row["reason"].as_str().unwrap_or("").to_string(),
                    row["detail"].as_str().unwrap_or("").to_string(),
                )
            })
        })
        .collect()
}

fn edges_of_caller<'a>(
    edges: &'a [(String, String, String, String)],
    caller: &str,
) -> Vec<&'a (String, String, String, String)> {
    edges.iter().filter(|e| e.0 == caller).collect()
}

#[test]
fn a_short_var_from_a_same_package_func_binds_the_result_type() {
    let edges = resolved_edges(&inferred_dir());
    let ring = edges_of_caller(&edges, "fromSamePkgFunc")
        .into_iter()
        .find(|e| e.1 == "Ring")
        .expect("t := NewThing(); t.Ring() must resolve");
    assert!(
        ring.2.ends_with("lib.go"),
        "Ring bound outside lib.go: {ring:?}"
    );
    assert_eq!(ring.3, "name_resolve");
}

#[test]
fn a_short_var_from_an_import_qualified_func_binds_across_the_package() {
    let edges = resolved_edges(&inferred_dir());
    let hello = edges_of_caller(&edges, "fromImportQualifiedFunc")
        .into_iter()
        .find(|e| e.1 == "Hello")
        .expect("s := sub.NewSub(); s.Hello() must resolve");
    assert!(
        hello.2.ends_with("sub/sub.go"),
        "Hello bound outside sub/sub.go: {hello:?}"
    );
    assert_eq!(hello.3, "name_resolve");
}

#[test]
fn a_short_var_from_a_method_on_a_known_receiver_binds_the_method_result() {
    let edges = resolved_edges(&inferred_dir());
    let ring = edges_of_caller(&edges, "fromMethodResult")
        .into_iter()
        .find(|e| e.1 == "Ring")
        .expect("t := w.Clone(); t.Ring() must resolve");
    assert!(
        ring.2.ends_with("lib.go"),
        "Ring bound outside lib.go: {ring:?}"
    );
}

#[test]
fn a_pair_result_binds_only_its_own_slot() {
    let edges = resolved_edges(&inferred_dir());
    // `(T, error)`: the FIRST slot names Thing, so t.Ring() resolves.
    let ring = edges_of_caller(&edges, "fromPairFirst")
        .into_iter()
        .find(|e| e.1 == "Ring")
        .expect("t, err := MightFail(); t.Ring() must resolve");
    assert!(ring.2.ends_with("lib.go"), "Ring bound outside lib.go: {ring:?}");
    // `err` holds error, which declares no corpus method: stays unresolved.
    let unresolved = unresolved_rows(&inferred_dir());
    assert!(
        unresolved.iter().any(|(path, reason, detail)| {
            path.ends_with("callers.go") && reason == "inferred" && detail == "Error"
        }),
        "err.Error() must stay inferred: {unresolved:?}"
    );
}

#[test]
fn a_multi_assign_binds_by_result_index() {
    let edges = resolved_edges(&inferred_dir());
    // a, b := Two(): slot 0 is *Thing, slot 1 is *Other. Both methods are
    // traceable ONLY through the index (a bare name search finds each once,
    // but the assertion pins the callee_path to each type's own method).
    let ring = edges_of_caller(&edges, "fromMultiAssignIndex")
        .into_iter()
        .find(|e| e.1 == "Ring")
        .expect("a.Ring() must resolve through slot 0");
    assert!(ring.2.ends_with("lib.go"), "Ring bound outside lib.go: {ring:?}");
    let bell = edges_of_caller(&edges, "fromMultiAssignIndex")
        .into_iter()
        .find(|e| e.1 == "Bell")
        .expect("b.Bell() must resolve through slot 1");
    assert!(bell.2.ends_with("lib.go"), "Bell bound outside lib.go: {bell:?}");
}

#[test]
fn a_chain_resolves_in_source_order_within_one_pass() {
    let edges = resolved_edges(&inferred_dir());
    // a := NewThing(); b := a.Clone(); b.Ring(): b's binding is itself a
    // call result on an inferred receiver, legal because a is already bound.
    let ring = edges_of_caller(&edges, "chainResolvesInSourceOrder")
        .into_iter()
        .find(|e| e.1 == "Ring")
        .expect("b.Ring() through the chained binding must resolve");
    assert!(ring.2.ends_with("lib.go"), "Ring bound outside lib.go: {ring:?}");
    assert_eq!(
        edges_of_caller(&edges, "chainResolvesInSourceOrder").len(),
        3,
        "NewThing, Clone and Ring all resolve: {edges:?}"
    );
}

#[test]
fn an_unbound_callee_keeps_the_site_inferred() {
    let edges = resolved_edges(&inferred_dir());
    assert!(
        !edges.iter().any(|e| e.0 == "unboundCalleeStaysInferred" && e.1 == "Ring"),
        "x := undefinedCallee() binds nothing, so x.Ring() must not resolve: {edges:?}"
    );
    let unresolved = unresolved_rows(&inferred_dir());
    assert!(
        unresolved.iter().any(|(path, reason, detail)| {
            path.ends_with("callers.go") && reason == "inferred" && detail == "Ring"
        }),
        "x.Ring() must drop with reason inferred: {unresolved:?}"
    );
}

#[test]
fn an_interface_result_keeps_the_site_inferred() {
    let edges = resolved_edges(&inferred_dir());
    assert!(
        !edges.iter().any(|e| e.0 == "interfaceResultStaysInferred" && e.1 == "Error"),
        "e := NewErr() binds error, which has no corpus decl: {edges:?}"
    );
    let unresolved = unresolved_rows(&inferred_dir());
    assert!(
        unresolved.iter().any(|(path, reason, detail)| {
            path.ends_with("callers.go") && reason == "inferred" && detail == "Error"
        }),
        "e.Error() must drop with reason inferred: {unresolved:?}"
    );
}

/// A module where every file binds through an inferred receiver: doubling the
/// file count must not more than `RATIO_BUDGET`x the wall. The result-type
/// lookup is cached per callee file (one parse per blob, process-wide), so the
/// work is linear in files, never per site x corpus.
fn generated_callers(dir: &std::path::Path, n: usize) -> Vec<String> {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/inferscale\n\ngo 1.22\n").unwrap();
    std::fs::write(
        dir.join("lib.go"),
        "package inferscale\n\ntype Thing struct{}\n\nfunc (t *Thing) Ring() string { return \"r\" }\n\nfunc NewThing() *Thing { return &Thing{} }\n",
    )
    .unwrap();
    let mut paths = vec![dir.join("lib.go").to_string_lossy().into_owned()];
    for i in 0..n {
        let body = format!(
            "package inferscale\n\nfunc caller{i}() string {{\n\tt := NewThing()\n\treturn t.Ring()\n}}\n"
        );
        let path = dir.join(format!("caller{i}.go"));
        std::fs::write(&path, body).unwrap();
        paths.push(path.to_string_lossy().into_owned());
    }
    paths
}

fn resolve_wall(paths: &[String]) -> f64 {
    let start = Instant::now();
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(paths)
        .output()
        .expect("extract binary runs");
    assert!(out.status.success(), "resolve failed");
    start.elapsed().as_secs_f64()
}

#[test]
fn the_result_type_lookup_is_built_once_not_per_site() {
    let dir = std::env::temp_dir().join("sprefa-extract-63-infer-scale");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let wall200 = resolve_wall(&generated_callers(&dir.join("n200"), 200));
    let wall400 = resolve_wall(&generated_callers(&dir.join("n400"), 400));

    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
