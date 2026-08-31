//! The ts receiver-typed member-call leg: `x.f()` with `x` bound to a named
//! type (param annotation, `const x: T`, class field, `this` inside a class,
//! `new T()`, one hop through a `const x = f()` initializer) binds `T.f` from
//! the declaring class/interface, `extends` one hop up. Union receivers stay
//! ambiguous, literal-inferred receivers stay inferred, and a member missing
//! on the receiver's type is a drop, never a name-match fallback.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::process::Command;
use std::time::Instant;

const RATIO_BUDGET: f64 = 2.5;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn caller_files() -> Vec<String> {
    let dir = fixture("ts5_findings/member_calls");
    vec![format!("{dir}/classes.ts"), format!("{dir}/callers.ts")]
}

fn resolve() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(caller_files())
        .output()
        .expect("extract binary runs");
    assert!(
        out.status.success(),
        "resolve failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("utf8 wire")
}

fn resolved_edges() -> Vec<(String, String, String, String)> {
    resolve()
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

fn unresolved_rows() -> Vec<(String, String, String)> {
    resolve()
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
fn a_param_annotation_binds_the_class_member() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "fromParam")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("r.load() on a RingRepo param must resolve");
    assert!(
        load.2.ends_with("classes.ts"),
        "load must bind in classes.ts: {load:?}"
    );
    assert_eq!(load.3, "name_resolve");
}

#[test]
fn a_const_annotation_binds_the_class_member() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "fromConstAnnot")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("r.load() on a `const r: RingRepo` must resolve");
    assert!(load.2.ends_with("classes.ts"), "load bound outside classes.ts");
}

#[test]
fn this_inside_a_class_binds_the_own_method() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "go")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("this.load() inside ThisUser must resolve");
    assert!(load.2.ends_with("callers.ts"), "load bound outside callers.ts");
}

#[test]
fn a_new_expression_receiver_binds_the_constructed_class_member() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "fromNew")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("new RingRepo().load() must resolve");
    assert!(load.2.ends_with("classes.ts"), "load bound outside classes.ts");
}

#[test]
fn a_static_receiver_binds_the_static_member() {
    let edges = resolved_edges();
    let unload = edges_of_caller(&edges, "fromStatic")
        .into_iter()
        .find(|e| e.1 == "unload")
        .expect("RingRepo.unload() must resolve");
    assert!(unload.2.ends_with("classes.ts"), "unload bound outside classes.ts");
}

#[test]
fn a_class_field_receiver_binds_the_field_type_member() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "fromField")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("holder.repo.load() must resolve through the field's type");
    assert!(load.2.ends_with("classes.ts"), "load bound outside classes.ts");
    let ping = edges_of_caller(&edges, "fromThisField")
        .into_iter()
        .find(|e| e.1 == "ping")
        .expect("holder.other.ping() must resolve through the field's type");
    assert!(ping.2.ends_with("classes.ts"), "ping bound outside classes.ts");
}

#[test]
fn an_interface_receiver_binds_the_interface_member() {
    let edges = resolved_edges();
    let ping = edges_of_caller(&edges, "fromIface")
        .into_iter()
        .find(|e| e.1 == "ping")
        .expect("r.ping() on a RepoIface param must resolve");
    assert!(ping.2.ends_with("classes.ts"), "ping bound outside classes.ts");
}

#[test]
fn an_interface_extends_hop_binds_the_base_member() {
    let edges = resolved_edges();
    let ping = edges_of_caller(&edges, "fromIfaceExtends")
        .into_iter()
        .find(|e| e.1 == "ping")
        .expect("r.ping() through ExtIface extends one hop must resolve");
    assert!(ping.2.ends_with("classes.ts"));
}

#[test]
fn a_class_extends_hop_binds_the_inherited_member() {
    let edges = resolved_edges();
    let ping = edges_of_caller(&edges, "fromClassExtends")
        .into_iter()
        .find(|e| e.1 == "ping")
        .expect("r.ping() on ExtRepo must resolve through the one-hop base");
    assert!(ping.2.ends_with("classes.ts"), "ping bound outside classes.ts");
}

#[test]
fn a_const_from_a_call_binds_through_the_declared_return_type() {
    let edges = resolved_edges();
    let load = edges_of_caller(&edges, "fromOneHop")
        .into_iter()
        .find(|e| e.1 == "load")
        .expect("const r = makeRing(); r.load() must resolve through the one hop");
    assert!(load.2.ends_with("classes.ts"), "load bound outside classes.ts");
}

#[test]
fn a_union_receiver_stays_ambiguous() {
    let edges = resolved_edges();
    assert!(
        !edges
            .iter()
            .any(|e| e.0 == "fromUnion" && e.1 == "load"),
        "a.load() on a union receiver must stay ambiguous: {edges:?}"
    );
    let unresolved = unresolved_rows();
    assert!(
        unresolved
            .iter()
            .any(|(path, reason, detail)| { path.ends_with("callers.ts") && reason == "ambiguous" && detail == "load" }),
        "a.load() must drop with reason ambiguous: {unresolved:?}"
    );
}

#[test]
fn a_member_missing_on_the_receiver_type_binds_nothing() {
    let edges = resolved_edges();
    assert!(
        !edges.iter().any(|e| e.0 == "fromMissingMember"),
        "Empty declares no ghostMethod: no edge, no name-match fallback: {edges:?}"
    );
}

/// Doubling the corpus must not more than `RATIO_BUDGET`x the wall: the
/// receiver table is ONE pass per body, and the per-file facts parse once per
/// blob, process-wide.
fn resolve_wall(paths: &[String]) -> f64 {
    // Min of 3 runs: at ~20 ms absolute walls, scheduler jitter from the
    // parallel test suite can inflate one run past RATIO_BUDGET; the minimum
    // is the noise-free wall the linear-growth assertion needs.
    (0..3)
        .map(|_| {
            let start = Instant::now();
            let out = Command::new(env!("CARGO_BIN_EXE_extract"))
                .arg("--resolve")
                .args(paths)
                .output()
                .expect("extract binary runs");
            assert!(out.status.success(), "resolve failed");
            start.elapsed().as_secs_f64()
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn the_receiver_table_is_one_pass_per_body() {
    let dir = std::env::temp_dir().join("sprefa-extract-65-receiver-scale");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let wall200 = resolve_wall(&generated_callers(&dir.join("n200"), 200));
    let wall400 = resolve_wall(&generated_callers(&dir.join("n400"), 400));

    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}

fn generated_callers(dir: &std::path::Path, n: usize) -> Vec<String> {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("lib.ts"),
        "export class ScaleRepo {\n  load(): number {\n    return 1;\n  }\n}\n",
    )
    .unwrap();
    let mut paths = vec![dir.join("lib.ts").to_string_lossy().into_owned()];
    for i in 0..n {
        let body = format!(
            "import {{ ScaleRepo }} from './lib';\nexport function caller{i}(): number {{\n  const r: ScaleRepo = new ScaleRepo();\n  return r.load();\n}}\n"
        );
        let path = dir.join(format!("caller{i}.ts"));
        std::fs::write(&path, body).unwrap();
        paths.push(path.to_string_lossy().into_owned());
    }
    paths
}
