//! Go package-qualified call sites: the import block is what turns `pkg.F(...)`
//! into a resolvable coordinate. Phase 1 mints `callee_path` = the import path
//! the receiver name binds; a selector on anything else keeps `callee_path`
//! None (receiver typing is out of scope for the syntactic tier).
//!
//! Expected values are hand-derived from the fixture, never copied from the
//! extractor's output.

use std::process::Command;
use std::time::Instant;

use sprefa_extract::{FamilyMask, GoSource, Source};

/// `tests/46_resolve_scaling.rs`'s budget, on the import-qualified shape.
const RATIO_BUDGET: f64 = 2.5;

const PATH: &str = "tests/fixtures/go_findings/pkg_qualified_calls.go";
const SOURCE: &[u8] = include_bytes!("fixtures/go_findings/pkg_qualified_calls.go");

/// `(callee, callee_path)` per call site, in tree order.
fn sites(path: &str, source: &[u8]) -> Vec<(String, Option<String>)> {
    let output = GoSource.extract(path, source, FamilyMask::ALL);
    let call = output.call.as_ref().expect("go mints a call plane");
    call.aux
        .sites
        .iter()
        .map(|site| {
            (
                output.strings.lookup(site.callee).to_string(),
                site.callee_path
                    .map(|id| output.strings.lookup(id).to_string()),
            )
        })
        .collect()
}

/// The whole site table of the fixture. Every import form appears once:
///   `alpha.Helper()`    plain spec, the LAST SEGMENT binds  -> the spec string
///   `a2.Only()`         aliased spec, the ALIAS binds       -> the spec string
///   `strings.TrimSpace` a stdlib path is still an import    -> the spec string
///   `b.Method()`        receiver is a value, not an import  -> None
///   `Dotted()`          a dot-import binds no qualifier     -> None
///   `side.Skipped()`    a blank import binds nothing        -> None
///   `alpha.inner.Deep()` receiver is a selector, not a name -> None
///   `local()`           a bare same-package call            -> None
#[test]
fn import_qualified_sites_carry_the_import_path() {
    let rows = sites(PATH, SOURCE);
    let expected: Vec<(String, Option<String>)> = [
        ("Helper", Some("example.com/m/alpha")),
        ("Only", Some("example.com/m/beta")),
        ("TrimSpace", Some("strings")),
        ("Method", None),
        ("Dotted", None),
        ("Skipped", None),
        ("Deep", None),
        ("local", None),
    ]
    .iter()
    .map(|(callee, path)| (callee.to_string(), path.map(str::to_string)))
    .collect();

    assert_eq!(rows, expected);
}

/// A file with no import block mints the same sites it always did.
#[test]
fn a_file_without_imports_mints_no_callee_path() {
    let source = b"package p\n\nfunc a() int { return b() }\n\nfunc b() int { return 1 }\n";
    let rows = sites("tests/fixtures/go_findings/none.go", source);

    assert_eq!(rows, [("b".to_string(), None)]);
}

/// The callee name itself is untouched by the path: `pkg.F` still keys on `F`,
/// which is what the same-package name-match joins on.
#[test]
fn the_callee_stays_the_trailing_name() {
    let rows = sites(PATH, SOURCE);
    let helper = rows.first().expect("the first site is alpha.Helper");

    assert_eq!(helper.0, "Helper");
}

/// `(caller_name, callee_name, callee_path)` per resolved edge of one
/// `--resolve` run over `paths`.
fn resolved_edges(paths: &[String]) -> Vec<(String, String, String)> {
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
                )
            })
        })
        .collect()
}

fn fixture(rel: &str) -> String {
    format!("{}/tests/fixtures/{rel}", env!("CARGO_MANIFEST_DIR"))
}

/// A generated module under `dir`: packages alpha and beta both export
/// `Helper{i}`, only beta exports `Only{i}`, and gamma calls both through its
/// imports. `pairs` counts the `(Helper, Only)` pairs, so gamma writes
/// `2 * pairs` cross-package calls, half of them on an ambiguous name.
fn generated_module(dir: &std::path::Path, pairs: usize) -> Vec<String> {
    for package in ["alpha", "beta", "gamma"] {
        std::fs::create_dir_all(dir.join(package)).unwrap();
    }
    std::fs::write(dir.join("go.mod"), "module example.com/gen\n\ngo 1.22\n").unwrap();

    let mut alpha = String::from("package alpha\n\n");
    let mut beta = String::from("package beta\n\n");
    let mut gamma = String::from(
        "package gamma\n\nimport (\n\t\"strings\"\n\n\t\"example.com/gen/alpha\"\n\tb \"example.com/gen/beta\"\n)\n\n",
    );
    for i in 0..pairs {
        alpha.push_str(&format!("func Helper{i}() int {{ return {i} }}\n"));
        beta.push_str(&format!("func Helper{i}() int {{ return {i} }}\n"));
        beta.push_str(&format!("func Only{i}() int {{ return {i} }}\n"));
        gamma.push_str(&format!(
            "func CallA{i}() int {{ return alpha.Helper{i}() }}\n"
        ));
        gamma.push_str(&format!("func CallB{i}() int {{ return b.Only{i}() }}\n"));
    }
    // An external import naming a callee the corpus DOES declare: `strings` is
    // outside the module, so `strings.Only0` has no corpus target at all.
    gamma.push_str("func CallExternal() int { return strings.Only0() }\n");
    let files = [
        (dir.join("alpha/alpha.go"), alpha),
        (dir.join("beta/beta.go"), beta),
        (dir.join("gamma/gamma.go"), gamma),
    ];
    files
        .into_iter()
        .map(|(path, text)| {
            std::fs::write(&path, text).unwrap();
            path.to_string_lossy().into_owned()
        })
        .collect()
}

/// 40 cross-package calls, 20 of them on a name BOTH packages export. All 40
/// resolve, each into the package its import names: `alpha.Helper3` picks
/// alpha's, `b.Only3` picks beta's, and nothing lands back in gamma.
#[test]
fn every_cross_package_call_resolves_into_the_package_the_import_names() {
    let dir = std::env::temp_dir().join("sprefa-extract-51-cross-package");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let paths = generated_module(&dir, 20);
    let edges = resolved_edges(&paths);

    let helper: Vec<&(String, String, String)> = edges
        .iter()
        .filter(|e| e.0.starts_with("CallA") && e.1.starts_with("Helper"))
        .collect();
    let only: Vec<&(String, String, String)> = edges
        .iter()
        .filter(|e| e.0.starts_with("CallB") && e.1.starts_with("Only"))
        .collect();
    assert_eq!(
        (helper.len(), only.len()),
        (20, 20),
        "40 cross-package calls, 40 edges: ambiguous {} unique {}",
        helper.len(),
        only.len()
    );
    for edge in &helper {
        assert!(
            edge.2.ends_with("alpha/alpha.go"),
            "alpha.Helper landed outside alpha: {edge:?}"
        );
    }
    for edge in &only {
        assert!(
            edge.2.ends_with("beta/beta.go"),
            "b.Only landed outside beta: {edge:?}"
        );
    }
    assert!(
        !edges.iter().any(|e| e.2.ends_with("gamma/gamma.go")),
        "a cross-package call bound the calling package: {edges:?}"
    );
}

/// `strings.Only0()` names a package outside the module, so it has no corpus
/// target however many `Only0`s the corpus declares. A unique corpus name is
/// not evidence that the imported package is the one that declares it.
#[test]
fn a_call_through_an_external_import_resolves_to_nothing() {
    let dir = std::env::temp_dir().join("sprefa-extract-51-external");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let paths = generated_module(&dir, 20);
    let edges = resolved_edges(&paths);

    let external: Vec<&(String, String, String)> = edges
        .iter()
        .filter(|edge| edge.0 == "CallExternal")
        .collect();

    assert!(
        external.is_empty(),
        "a stdlib-qualified call bound a corpus def: {external:?}"
    );
}

/// A file that declares its own `Helper` and calls `alpha.Helper()` binds the
/// call in package alpha. The same-file name-match is the wrong candidate set
/// for an import-qualified site, whatever the file happens to declare.
#[test]
fn an_import_qualified_call_never_binds_the_callers_own_def() {
    let paths = vec![
        fixture("go_findings/own_name_shadow/alpha/alpha.go"),
        fixture("go_findings/own_name_shadow/caller/caller.go"),
    ];
    let edges = resolved_edges(&paths);
    let run = edges
        .iter()
        .find(|(caller, _, _)| caller == "Run")
        .expect("Run's call to alpha.Helper resolves");

    assert_eq!(run.1, "Helper");
    assert!(
        run.2.ends_with("own_name_shadow/alpha/alpha.go"),
        "alpha.Helper bound the caller's own Helper: {}",
        run.2
    );
}

/// One caller file per exported `Helper{i}`, each reaching it through the
/// import. `n` files, `n + 1` in the resolve universe.
fn qualified_module(dir: &std::path::Path, n: usize) -> Vec<String> {
    std::fs::create_dir_all(dir.join("alpha")).unwrap();
    std::fs::create_dir_all(dir.join("caller")).unwrap();
    std::fs::write(dir.join("go.mod"), "module example.com/scale\n\ngo 1.22\n").unwrap();

    let mut alpha = String::from("package alpha\n\n");
    for i in 0..n {
        alpha.push_str(&format!("func Helper{i}() int {{ return {i} }}\n"));
    }
    let alpha_path = dir.join("alpha/alpha.go");
    std::fs::write(&alpha_path, alpha).unwrap();

    let mut paths = vec![alpha_path.to_string_lossy().into_owned()];
    for i in 0..n {
        let path = dir.join(format!("caller/f{i}.go"));
        std::fs::write(
            &path,
            format!(
                "package caller\n\nimport \"example.com/scale/alpha\"\n\nfunc local{i}() int {{ return {i} }}\n\nfunc use{i}() int {{ return alpha.Helper{i}() + local{i}() }}\n"
            ),
        )
        .unwrap();
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

/// The imported leg costs one own-blob join per FILE, never one per call site;
/// doubling the file count must not more than double the wall.
#[test]
fn resolve_wall_grows_linearly_over_import_qualified_files() {
    let dir = std::env::temp_dir().join("sprefa-extract-51-scale");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let wall200 = resolve_wall(&qualified_module(&dir.join("n200"), 200));
    let wall400 = resolve_wall(&qualified_module(&dir.join("n400"), 400));

    assert!(
        wall400 / wall200 < RATIO_BUDGET,
        "wall(400)={wall400:.3}s vs wall(200)={wall200:.3}s exceeds {RATIO_BUDGET}x"
    );
}
