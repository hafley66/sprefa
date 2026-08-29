//! Go package-qualified call sites: the import block is what turns `pkg.F(...)`
//! into a resolvable coordinate. Phase 1 mints `callee_path` = the import path
//! the receiver name binds; a selector on anything else keeps `callee_path`
//! None (receiver typing is out of scope for the syntactic tier).
//!
//! Expected values are hand-derived from the fixture, never copied from the
//! extractor's output.

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
