//! A method's receiver-declared type parameters (`func (g Gen[T]) Get() T`)
//! must be excluded from its sigs, same as type_parameters-declared ones.

use sprefa_extract::{FamilyMask, GoSource, Source};

const PATH: &str = "tests/fixtures/go/corpus_1.go";
const SOURCE: &[u8] = include_bytes!("fixtures/go/corpus_1.go");

#[test]
fn go_receiver_type_params_excluded_from_sigs() {
    let output = GoSource.extract(PATH, SOURCE, FamilyMask::ALL);
    let types = output.types.as_ref().unwrap();

    let sig_tys: Vec<&str> = types
        .aux
        .sigs
        .iter()
        .map(|sig| output.strings.lookup(sig.ty))
        .collect();

    assert!(
        !sig_tys.contains(&"T"),
        "receiver type parameter leaked into sigs: {sig_tys:?}"
    );
}
