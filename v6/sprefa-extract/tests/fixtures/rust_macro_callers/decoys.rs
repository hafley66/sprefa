// Same-named callees so a corpus-wide unique-name match cannot bind the
// minted sites by name alone (tests/93 discipline): only the same-file leg
// picks user.rs's own defs.
pub fn helper_one() -> u32 {
    10
}
pub fn helper_two() -> u32 {
    20
}
