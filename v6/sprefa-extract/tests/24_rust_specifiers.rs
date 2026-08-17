//! Rust module-specifier test: drives `RustSource` directly. Expected values
//! are hand-derived from `sample.rs`, never copied from the extractor's output.

use sprefa_extract::{FamilyMask, RustSource, Source};

const PATH: &str = "tests/fixtures/rust_modules/sample.rs";
const SOURCE: &[u8] = include_bytes!("fixtures/rust_modules/sample.rs");

/// SABOTAGE RECEIPT: flipping `use_tree_leaves`'s Glob arm to `SpecifierKind::Named`
/// leaves `theta` claiming a named binding and this test fails on row 7; dropping the
/// `mod_path_attr` lookup makes row 13's module read `sigma` instead of `rho.rs`.
#[test]
fn rust_use_and_mod_specifiers() {
    let output = RustSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let rows: Vec<(&str, &str, Option<&str>, u32)> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| {
            (
                specifier.kind.as_str(),
                output.strings.lookup(specifier.name),
                specifier.module.map(|id| output.strings.lookup(id)),
                specifier.span.start,
            )
        })
        .collect();

    assert_eq!(
        rows,
        [
            ("named", "beta", Some("alpha::beta"), 167),
            ("named", "gee", Some("alpha::gamma"), 184),
            ("named", "delta", Some("alpha::delta"), 210),
            ("named", "epsilon", Some("alpha::epsilon"), 217),
            ("named", "zeta", Some("alpha::zeta"), 245),
            ("named", "eta", Some("alpha::eta"), 399),
            ("namespace", "theta", Some("theta"), 416),
            ("reexport", "kappa", Some("iota::kappa"), 433),
            ("reexport", "mu", Some("lambda::mu"), 463),
            ("reexport", "nu", Some("nu"), 479),
            ("named", "omicron", Some("omicron"), 501),
            ("named", "pi", Some("pi"), 514),
            ("named", "sigma", Some("rho.rs"), 527),
            ("named", "phi", Some("upsilon::phi"), 585),
        ]
    );
}

/// `extern crate xi;` names no module edge, and an inline `mod tau { .. }` names no
/// other file, so neither gets a row; the `use` inside `tau` still does.
#[test]
fn rust_extern_crate_and_inline_mod_get_no_row() {
    let output = RustSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let names: Vec<&str> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| output.strings.lookup(specifier.name))
        .collect();
    let modules: Vec<&str> = call
        .aux
        .specifiers
        .iter()
        .filter_map(|specifier| specifier.module.map(|id| output.strings.lookup(id)))
        .collect();

    assert!(
        !names.contains(&"xi"),
        "extern crate emitted a row: {names:?}"
    );
    assert!(
        !modules.contains(&"xi"),
        "extern crate emitted a row: {modules:?}"
    );
    assert!(
        !names.contains(&"tau"),
        "inline mod emitted a row: {names:?}"
    );
    assert!(
        !modules.contains(&"tau"),
        "inline mod emitted a row: {modules:?}"
    );
    assert!(
        names.contains(&"phi"),
        "inline mod body was not walked: {names:?}"
    );
}
