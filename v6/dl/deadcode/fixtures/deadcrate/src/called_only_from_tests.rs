// Five shipped defs whose only call site sits in another file's `#[cfg(test)]
// mod tests`. rustc: FIRES, `cargo check` is a non-test build and never reaches
// them. rail: MUST FIRE. The def plane already subtracts cfg-guarded defs; a
// call site under the same predicate has to be subtracted on the call plane or
// a test helper in live_pub.rs keeps this file alive on its own.
pub(crate) fn fixture_step_one() -> usize { fixture_step_two() }
fn fixture_step_two() -> usize { fixture_step_three() }
fn fixture_step_three() -> usize { fixture_step_four() }
fn fixture_step_four() -> usize { fixture_step_five() }
fn fixture_step_five() -> usize { 5 }
