//! Go module-specifier test: drives `GoSource` directly. Expected values are
//! hand-derived from `sample.go`, never copied from the extractor's output.

use sprefa_extract::{FamilyMask, GoSource, Source};

const PATH: &str = "tests/fixtures/go_modules/sample.go";
const SOURCE: &[u8] = include_bytes!("fixtures/go_modules/sample.go");

#[test]
fn go_import_specifiers() {
    let output = GoSource.extract(PATH, SOURCE, FamilyMask::ALL);
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
            ("named", "fmt", None, 142),
            ("named", "os", None, 159),
            ("named", "alias", Some("path/filepath"), 165),
            ("side_effect", "embed", None, 188),
            ("namespace", "strings", None, 199),
        ]
    );
}

/// The `package sample` clause is not an import, so it gets no row.
#[test]
fn go_package_clause_is_not_a_specifier() {
    let output = GoSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let names: Vec<&str> = call
        .aux
        .specifiers
        .iter()
        .map(|specifier| output.strings.lookup(specifier.name))
        .collect();

    assert!(
        !names.contains(&"sample"),
        "package clause emitted a row: {names:?}"
    );
}
