// qualified_path/util/mod.rs: a `mod.rs` names its directory, so this file's
// module path ends with `util` and `util::helper()` in main.rs binds HERE, not
// in util/deep.rs, which also defines `helper`.
//
// EXPECTED: no outgoing edge; `deep::helper` is exercised from main.rs.

pub fn helper() -> u32 {
    5
}
