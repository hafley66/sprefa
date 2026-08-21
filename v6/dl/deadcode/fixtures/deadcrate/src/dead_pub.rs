// rustc: SILENT. A pub item in a lib crate is assumed used downstream, so the
// dead_code lint can never reach these. rail: MUST FIRE. This is the case the
// rail exists for and the one rustc is structurally unable to see.
pub fn orphan_pub_one() -> usize { orphan_pub_two() }
pub fn orphan_pub_two() -> usize { orphan_pub_three() }
pub fn orphan_pub_three() -> usize { orphan_pub_four() }
pub fn orphan_pub_four() -> usize { orphan_pub_five() }
pub fn orphan_pub_five() -> usize { 3 }
