//! The go type plane: receiver-typed method dispatch (leg 1), interface
//! dispatch (leg 2), and builtins (leg 3). Every case here would previously
//! resolve wrong, ambiguously, or not at all: `Name`/`Speak`/`Volume` are
//! deliberately non-unique corpus names, so a passing assertion proves the
//! declared-type narrowing did the work, not a lucky unique name.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
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

fn type_plane_dir() -> Vec<String> {
    let dir = fixture("go_findings/type_plane");
    let mut paths: Vec<String> = std::fs::read_dir(&dir)
        .expect("type_plane dir readable")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "go"))
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    paths.sort();
    paths
}

/// Every call in receivers.go names a bare method: `w.Name()` where w's
/// declared type is Widget, never Gadget (the competing same-name method in
/// gadget.go). Same-package method lookup covers local var, param, pointer
/// (a `&Widget{}` composite literal), one struct field, and one slice
/// element. Without leg 1, "Name" is ambiguous corpus-wide and NONE of these
/// resolve.
#[test]
fn receiver_typed_dispatch_covers_var_param_pointer_field_and_slice() {
    let edges = resolved_edges(&type_plane_dir());
    let name_edges: Vec<&(String, String, String, String)> =
        edges.iter().filter(|e| e.1 == "Name").collect();
    let callers: std::collections::BTreeSet<&str> =
        name_edges.iter().map(|e| e.0.as_str()).collect();
    assert_eq!(
        callers,
        std::collections::BTreeSet::from([
            "localVar",
            "viaParam",
            "viaPointer",
            "viaField",
            "viaSliceElement",
            "viaInferred",
        ]),
        "receiver-typed callers with a Name edge: {name_edges:?}"
    );
    for edge in &name_edges {
        assert!(
            edge.2.ends_with("receivers.go"),
            "{} bound Name outside receivers.go (into gadget.go?): {edge:?}",
            edge.0
        );
        assert_eq!(edge.3, "name_resolve");
    }
}

/// `w := newWidget(); w.Name()`: the := binds from a call result. The
/// inferred-receiver plane (tests/63) gives w newWidget's declared result
/// type Widget, so the Name edge lands in receivers.go and no `inferred`
/// drop remains for it.
#[test]
fn a_call_result_receiver_type_binds_through_the_callee_result() {
    let paths = type_plane_dir();
    let edges = resolved_edges(&paths);
    let name = edges
        .iter()
        .find(|e| e.0 == "viaInferred" && e.1 == "Name")
        .expect("viaInferred resolves Name through newWidget's result type");
    assert!(
        name.2.ends_with("receivers.go"),
        "viaInferred's Name edge bound outside receivers.go: {name:?}"
    );
    assert_eq!(name.3, "name_resolve");
    let unresolved = unresolved_rows(&paths);
    assert!(
        !unresolved.iter().any(|(path, reason, detail)| {
            path.ends_with("receivers.go") && reason == "inferred" && detail == "Name"
        }),
        "the inferred Name drop must be gone: {unresolved:?}"
    );
}

/// `Speaker` (iface.go) has two full implementers (Loud, Quiet) and one
/// partial (Mute: no Volume). Every implements edge lands on the SPEC name
/// (`Speak`/`Volume`) with the implementer's OWN file as `callee_path`, so
/// counting distinct files proves both real implementers won and Mute did
/// not sneak in through its own (non-unique) `Speak` name.
#[test]
fn interface_dispatch_binds_every_full_implementer_and_skips_a_partial_one() {
    let edges = resolved_edges(&type_plane_dir());
    let implements: Vec<&(String, String, String, String)> =
        edges.iter().filter(|e| e.3 == "implements").collect();

    let speak_files: std::collections::BTreeSet<&str> = implements
        .iter()
        .filter(|e| e.0 == "Speak")
        .map(|e| e.2.as_str())
        .collect();
    let volume_files: std::collections::BTreeSet<&str> = implements
        .iter()
        .filter(|e| e.0 == "Volume")
        .map(|e| e.2.as_str())
        .collect();

    assert_eq!(
        speak_files.len(),
        2,
        "Speak should bind Loud and Quiet only: {speak_files:?}"
    );
    assert_eq!(
        volume_files.len(),
        2,
        "Volume should bind Loud and Quiet only: {volume_files:?}"
    );
    for path in speak_files.iter().chain(volume_files.iter()) {
        assert!(
            path.ends_with("loud.go") || path.ends_with("quiet.go"),
            "an implements edge landed outside loud.go/quiet.go: {path}"
        );
        assert!(
            !path.ends_with("mute.go"),
            "Mute implemented Speaker despite missing Volume: {path}"
        );
    }
}

/// `len(xs)` names the builtin: no local def named `len` exists in this
/// package, so the drop channel reports it as `builtin`, never a corpus gap.
#[test]
fn a_real_builtin_call_drops_with_reason_builtin() {
    let paths = type_plane_dir();
    let unresolved = unresolved_rows(&paths);
    let builtin: Vec<&(String, String, String)> = unresolved
        .iter()
        .filter(|(path, reason, detail)| {
            path.ends_with("builtins.go") && reason == "builtin" && detail == "len"
        })
        .collect();
    assert_eq!(
        builtin.len(),
        1,
        "expected exactly one builtin len drop: {unresolved:?}"
    );
}

/// A package-level `func len` shadows the builtin for the whole package: the
/// call resolves to that def, and no `builtin` row is ever emitted for it.
#[test]
fn a_package_level_func_named_len_shadows_the_builtin() {
    let dir = fixture("go_findings/type_plane_shadow");
    let paths = vec![format!("{dir}/shadow.go")];
    let edges = resolved_edges(&paths);
    let unresolved = unresolved_rows(&paths);

    assert!(
        edges
            .iter()
            .any(|e| e.0 == "caller" && e.1 == "len" && e.3 == "name_resolve"),
        "caller -> len (the shadow def) must resolve: {edges:?}"
    );
    assert!(
        !unresolved
            .iter()
            .any(|(_, reason, detail)| reason == "builtin" && detail == "len"),
        "the shadowed len must never drop as builtin: {unresolved:?}"
    );
}

/// COUNT: `viaField`, `viaSliceElement`, `viaParam`, `viaPointer`, `localVar`
/// each mint exactly one Name edge — one bound site per written call, no
/// duplicate or missing bindings from the receiver-typed leg.
#[test]
fn receiver_typed_edges_equal_the_fixtures_written_bindings() {
    let edges = resolved_edges(&type_plane_dir());
    let name_edge_count = edges.iter().filter(|e| e.1 == "Name").count();
    assert_eq!(
        name_edge_count, 6,
        "receivers.go writes 6 Name call sites through a traceable receiver (5 declared + viaInferred through the inferred binding)"
    );
}

/// A module with `n` types, all implementing a 2-method interface. Doubling
/// `n` must not more than `RATIO_BUDGET`x the wall: the implementer scan is
/// one pass per interface (grouped by spec name), never per call site.
fn generated_implementers_module(dir: &std::path::Path, n: usize) -> Vec<String> {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("go.mod"),
        "module example.com/implscale\n\ngo 1.22\n",
    )
    .unwrap();

    let mut iface =
        String::from("package implscale\n\ntype Iface interface {\n\tM1() int\n\tM2() int\n}\n\n");
    for i in 0..n {
        iface.push_str(&format!(
            "type T{i} struct{{}}\nfunc (t *T{i}) M1() int {{ return {i} }}\nfunc (t *T{i}) M2() int {{ return {i} }}\n"
        ));
    }
    let path = dir.join("iface.go");
    std::fs::write(&path, iface).unwrap();
    vec![path.to_string_lossy().into_owned()]
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
fn interface_implementer_scan_is_one_pass_not_per_site() {
    let dir = std::env::temp_dir().join("sprefa-extract-55-impl-scale");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let wall100 = resolve_wall(&generated_implementers_module(&dir.join("n100"), 100));
    let wall200 = resolve_wall(&generated_implementers_module(&dir.join("n200"), 200));

    assert!(
        wall200 / wall100 < RATIO_BUDGET,
        "wall(200)={wall200:.3}s vs wall(100)={wall100:.3}s exceeds {RATIO_BUDGET}x"
    );
}
