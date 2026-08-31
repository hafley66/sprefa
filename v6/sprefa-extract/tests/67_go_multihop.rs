//! The go multi-hop receiver chain: `a.b().c()` types the operand left to
//! right in one pass (a name's bound type, a struct field's declared type, a
//! method's declared first result, an import-qualified func's result) and
//! binds the final `.c()` the way #562 binds a one-hop receiver. The
//! interface fan-out applies when the last receiver type is an interface. A
//! hop landing on a builtin or a generic result stops the chain; past the
//! depth cap of 8 hops the chain stops too. Per site the walk is bounded by
//! the chain length, never a corpus scan.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.

use std::collections::BTreeSet;
use std::process::Command;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn multihop_dir() -> Vec<String> {
    let dir = fixture("go_findings/multihop");
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
/// `--resolve` run over the fixture dir.
fn resolved_edges() -> Vec<(String, String, String, String)> {
    let out = Command::new(env!("CARGO_BIN_EXE_extract"))
        .arg("--resolve")
        .args(multihop_dir())
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

fn callables<'a>(
    edges: &'a [(String, String, String, String)],
    caller: &str,
    callee: &str,
) -> Vec<&'a (String, String, String, String)> {
    edges
        .iter()
        .filter(|e| e.0 == caller && e.1 == callee)
        .collect()
}

/// `o.FS().FileExists()`: `FS` returns the struct `Host`, so the final
/// `FileExists` binds Host's method, not the interface spec's.
#[test]
fn through_a_struct_result() {
    let edges = resolved_edges();
    let hit = callables(&edges, "viaStruct", "FileExists");
    assert_eq!(hit.len(), 1, "one FileExists edge: {edges:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
    assert_eq!(hit[0].3, "name_resolve");
}

/// `o.VFS().FileExists()`: `VFS` returns the interface `FS`; the site keeps
/// the spec edge and fans out to every implementer of `FS`.
#[test]
fn through_an_interface_result_fans_out() {
    let edges = resolved_edges();
    let at_site = callables(&edges, "viaIface", "FileExists");
    let kinds: BTreeSet<&str> = at_site.iter().map(|e| e.3.as_str()).collect();
    assert_eq!(
        kinds,
        BTreeSet::from(["name_resolve", "implements"]),
        "spec edge plus fan-out: {at_site:?}"
    );
    let implements = at_site.iter().filter(|e| e.3 == "implements").count();
    assert_eq!(implements, 2, "Host and Real both fan out: {at_site:?}");
}

/// `a.cfg.Log.Write()`: two field hops through struct types declared in
/// another file of the package, then the method.
#[test]
fn through_fields_then_a_method() {
    let edges = resolved_edges();
    let hit = callables(&edges, "viaField", "Write");
    assert_eq!(hit.len(), 1, "one Write edge: {edges:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// `sub.NewSub().Hello()`: the root is an import-qualified func; its result
/// type carries the rest of the chain across the package.
#[test]
fn through_an_import_qualified_func() {
    let edges = resolved_edges();
    let hit = callables(&edges, "viaImport", "Hello");
    assert_eq!(hit.len(), 1, "one Hello edge: {edges:?}");
    assert!(
        hit[0].2.ends_with("sub/sub.go"),
        "bound in sub/sub.go: {hit:?}"
    );
}

/// `o.Name().ToUpper()`: `Name` returns the builtin `string`, which declares
/// no corpus method; the site keeps its current outcome, no edge.
#[test]
fn a_builtin_result_stops() {
    let edges = resolved_edges();
    assert!(
        callables(&edges, "viaBuiltin", "ToUpper").is_empty(),
        "no ToUpper edge: {edges:?}"
    );
}

/// `o.Items().Fetch()`: `Items` returns the generic `List[Item]`; the chain
/// stops at the generic result, so `Fetch` stays name-match-only, and
/// `Fetch` is declared twice (List, Host), an ambiguity the tier declines.
#[test]
fn a_generic_result_stops() {
    let edges = resolved_edges();
    assert!(
        callables(&edges, "viaGeneric", "Fetch").is_empty(),
        "no Fetch edge: {edges:?}"
    );
}

/// Eight hops (seven Self plus the final Ping) sit at the cap and resolve
/// onto `Orch.Ping`, never the ambiguous name match.
#[test]
fn eight_hops_at_the_cap_resolve() {
    let edges = resolved_edges();
    let hit = callables(&edges, "viaEight", "Ping");
    assert_eq!(hit.len(), 1, "one Ping edge: {edges:?}");
    assert!(hit[0].2.ends_with("lib.go"), "bound in lib.go: {hit:?}");
}

/// Nine hops exceed the cap: the chain stops and the site keeps its current
/// outcome, which here is the ambiguous two-def name match: no edge.
#[test]
fn depth_nine_stops() {
    let edges = resolved_edges();
    assert!(
        callables(&edges, "viaNine", "Ping").is_empty(),
        "no Ping edge past the cap: {edges:?}"
    );
}
