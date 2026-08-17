//! Kotlin module-specifier test: drives `KotlinSource` directly. Expected
//! values are hand-derived from `sample.kt`, never copied from the extractor's
//! output.

use sprefa_extract::{FamilyMask, KotlinSource, Source};

const PATH: &str = "tests/fixtures/kotlin_modules/sample.kt";
const SOURCE: &[u8] = include_bytes!("fixtures/kotlin_modules/sample.kt");

#[test]
fn kotlin_import_specifiers() {
    let output = KotlinSource.extract(PATH, SOURCE, FamilyMask::ALL);
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
            ("named", "List", Some("kotlin.collections.List"), 146),
            ("named", "JMap", Some("java.util.Map"), 177),
            ("namespace", "text", Some("kotlin.text"), 206),
        ]
    );
}

/// The `package sample` clause is not an import, so it gets no row.
#[test]
fn kotlin_package_clause_is_not_a_specifier() {
    let output = KotlinSource.extract(PATH, SOURCE, FamilyMask::ALL);
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
