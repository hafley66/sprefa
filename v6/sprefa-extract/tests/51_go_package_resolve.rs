//! Go package-qualified call sites: the import block is what turns `pkg.F(...)`
//! into a resolvable coordinate. Phase 1 mints `callee_path` = the import path
//! the receiver name binds; a selector on anything else keeps `callee_path`
//! None (receiver typing is out of scope for the syntactic tier).
//!
//! Expected values are hand-derived from the fixture, never copied from the
//! extractor's output.

use std::process::Command;

use sprefa_extract::{FamilyMask, GoSource, Source};

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
