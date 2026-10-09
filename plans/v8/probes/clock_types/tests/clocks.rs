#[test]
fn clock_rules() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/pass/*.rs");
    cases.compile_fail("tests/fail/*.rs");
}
