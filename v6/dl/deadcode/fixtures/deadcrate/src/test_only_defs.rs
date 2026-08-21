// Two real defs, five test-only ones, and NOTHING calls in from another file.
// rustc: silent, `published` is pub in a pub mod. rail: MUST STAY SILENT
// because two defs sit under the >= 5 floor. Delete the cfg filter and this
// file counts seven, clears the floor on test helpers alone, and reports dead.
// The call from lib.rs is deliberately absent: with it the file is live either
// way and the case stops discriminating.
pub fn published() -> usize { inner_step() }
fn inner_step() -> usize { 24 }

#[cfg(test)]
mod tests {
    #[test]
    fn checks_the_first_thing() { assert_eq!(super::published(), 24); }
    #[test]
    fn checks_the_second_thing() { assert_eq!(fixture(), 25); }
    #[test]
    fn checks_the_third_thing() { assert_eq!(fixture(), 25); }
    fn fixture() -> usize { 25 }
    fn unused_helper() -> usize { 26 }
}
