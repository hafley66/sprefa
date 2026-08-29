//! A `namespace` / `declare module` / `declare global` body reaches the type and
//! call families. Expected values are hand-derived from `corpus_1.ts`, never
//! copied from the extractor's output.

use sprefa_extract::{FamilyMask, Source, TsSource};

const PATH: &str = "tests/fixtures/ts/corpus_1.ts";
const SOURCE: &[u8] = include_bytes!("fixtures/ts/corpus_1.ts");

/// SABOTAGE RECEIPT: reverting `TypeProjector::project`'s loop to
/// `&program.body` leaves this list empty of everything but the two names the
/// file declares at depth 0, and `corpus_1.ts` declares none.
#[test]
fn module_block_declarations_reach_the_type_family() {
    let output = TsSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();

    let entities: Vec<(&str, Option<&str>)> = types
        .nodes
        .iter()
        .map(|node| {
            (
                node.kind.as_str(),
                node.name.map(|id| output.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(
        entities,
        [
            ("function", Some("nsFunc")),
            ("interface", Some("NsIface")),
            ("class", Some("NsClass")),
            ("method", Some("run")),
            ("alias", Some("NsAlias")),
            ("enum", Some("NsEnum")),
            ("function", Some("deepFn")),
            ("function", Some("ambientFn")),
            ("interface", Some("Window")),
        ]
    );
}

/// A string-enum nested in a namespace still carries its member values.
#[test]
fn module_block_enum_members_carry_their_const_values() {
    let output = TsSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();

    let consts: Vec<(Option<&str>, &str)> = types
        .aux
        .consts
        .iter()
        .map(|value| {
            (
                value.field.map(|id| output.strings.lookup(id)),
                output.strings.lookup(value.text),
            )
        })
        .collect();
    assert_eq!(consts, [(Some("Red"), "red")]);
}

/// Only the bodied declarations become call defs; `ambientFn` is bodiless, so it
/// carries none, exactly as a top-level `declare function` does.
#[test]
fn module_block_declarations_reach_the_call_family() {
    let output = TsSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let call = output.call.as_ref().unwrap();

    let defs: Vec<(&str, Option<&str>)> = call
        .nodes
        .iter()
        .map(|node| {
            (
                node.kind.as_str(),
                node.name.map(|id| output.strings.lookup(id)),
            )
        })
        .collect();
    assert_eq!(
        defs,
        [
            ("function", Some("nsFunc")),
            ("method", Some("run")),
            ("function", Some("deepFn")),
        ]
    );
}
