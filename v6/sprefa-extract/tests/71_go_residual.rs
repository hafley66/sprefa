//! The five residual go classes of the codeql-agreed set: a `type A = B` alias
//! receiver, a multi-hop chain whose hops are fields or an import-qualified
//! root, a one-hop receiver our scope never typed (range var, field read, index
//! read, multi-value define, type switch, conversion), a bare in-package call
//! whose name is not corpus-unique, and an import-qualified call a same-named
//! METHOD in the target directory was shadowing.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 71_go_residual`, 13 of 17):
//!   alias_receiver                    FAILED  one BasePing edge: []
//!   alias_chain                       FAILED  one BasePing edge: []
//!   field_hop_cross_package           FAILED  one BasePing edge: []
//!   field_hop_then_call               FAILED  one Make edge: []
//!   import_root_chain                 FAILED  one BasePing edge: []
//!   range_over_cross_package_field    FAILED  one BasePing edge: []
//!   range_over_channel                FAILED  one BasePing edge: []
//!   type_switch_case                  FAILED  one BasePing edge: []
//!   field_read_define                 FAILED  one BasePing edge: []
//!   index_read_define                 FAILED  one BasePing edge: []
//!   type_assertion_define             FAILED  one BasePing edge: []
//!   pointer_conversion                FAILED  one BasePing edge: []
//!   own_package_shadows_corpus_name   FAILED  one NewThing edge: []
//!   import_qualified_skips_method     FAILED  one IsThing edge: []
//!   multi_value_define                ok
//!   local_shadows_package_name        ok, by the import leg taking a METHOD
//!   range_index_only_declines         ok

use std::process::Command;

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

fn residual_dir() -> Vec<String> {
    let mut paths = walk(&fixture("go_residual"));
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
        .args(residual_dir())
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

/// One edge from `caller` to `callee`, landing in the file `file` names.
fn one_edge(caller: &str, callee: &str, file: &str) {
    let edges = resolved_edges();
    let hit = callables(&edges, caller, callee);
    assert_eq!(hit.len(), 1, "one {callee} edge: {hit:?}");
    assert!(hit[0].2.ends_with(file), "bound in {file}: {hit:?}");
}

/// `type Alias = Base`: `Base` owns the method, so the alias name only reaches
/// it through the alias table.
#[test]
fn alias_receiver() {
    one_edge("callAlias", "BasePing", "lib.go");
}

/// `type Hop = Alias = Base`: the walk follows the chain, not one hop.
#[test]
fn alias_chain() {
    one_edge("callAliasChain", "BasePing", "lib.go");
}

/// `h.One.BasePing()` where `Holder` lives in ANOTHER package: the field hop
/// resolves through the declaring package's own field table.
#[test]
fn field_hop_cross_package() {
    one_edge("callFieldHop", "BasePing", "lib.go");
}

/// `s.Gear.Make()`: two hops with no call before the last one, the shape the
/// old plan refused to record because it demanded a call hop.
#[test]
fn field_hop_then_call() {
    one_edge("callFieldHopTwice", "Make", "lib.go");
}

/// `lib.MakeBase().BasePing()`: an import-qualified root with zero hops before
/// the receiver, the other shape the call-hop demand refused.
#[test]
fn import_root_chain() {
    one_edge("callImportRoot", "BasePing", "lib.go");
}

/// `for _, item := range h.Items` over a cross-package slice field.
#[test]
fn range_over_cross_package_field() {
    one_edge("callRangeOverField", "BasePing", "lib.go");
}

/// `for item := range f.Stream` over a channel: ONE name takes the element.
#[test]
fn range_over_channel() {
    one_edge("callRangeOverChannel", "BasePing", "lib.go");
}

/// `for item := range h.Items` over a SLICE binds an index, never an element,
/// so the receiver stays untyped and no corpus-wide guess replaces it.
#[test]
fn range_index_only_declines() {
    let edges = resolved_edges();
    let hit = callables(&edges, "callRangeIndexOnly", "BasePing");
    assert!(hit.is_empty(), "a range index is no receiver: {hit:?}");
}

/// `switch narrowed := v.(type) { case *lib.Base: }` types the alias inside
/// that one case.
#[test]
fn type_switch_case() {
    one_edge("callTypeSwitch", "BasePing", "lib.go");
}

/// `read := h.One` on a cross-package struct: the rhs chain is recorded and
/// replayed where `read` is used.
#[test]
fn field_read_define() {
    one_edge("callFieldRead", "BasePing", "lib.go");
}

/// `read := h.Items[0]`: the index hop unwraps one collection level.
#[test]
fn index_read_define() {
    one_edge("callIndexRead", "BasePing", "lib.go");
}

/// `base, inner := lib.Pair()` types each name from its OWN result slot.
#[test]
fn multi_value_define() {
    one_edge("callMultiValue", "BasePing", "lib.go");
    one_edge("callMultiValue", "InnerPing", "lib.go");
}

/// `narrowed := v.(*lib.Base)` names its type outright.
#[test]
fn type_assertion_define() {
    one_edge("callTypeAssert", "BasePing", "lib.go");
}

/// `converted := (*Base)(m)` is a conversion, not a call; the parenthesized
/// spelling is what makes it one syntactically.
#[test]
fn pointer_conversion() {
    one_edge("callPointerConversion", "BasePing", "lib.go");
}

/// A bare `NewThing()` with `NewThing` also declared in `other`: the caller's
/// own package block binds it, where a corpus-wide unique-name leg declines.
#[test]
fn own_package_shadows_corpus_name() {
    one_edge("callOwnDirName", "NewThing", "lib.go");
}

/// `lib.IsThing()` with a same-named METHOD in `lib`: the free function is the
/// only candidate a qualified call may take.
#[test]
fn import_qualified_skips_method() {
    one_edge("callQualifiedFreeFunc", "IsThing", "lib.go");
}

/// `lib := lib.MakeBase()` shadows the package name; the local wins from the
/// next statement on.
#[test]
fn local_shadows_package_name() {
    one_edge("callThroughShadowedName", "BasePing", "lib.go");
}
