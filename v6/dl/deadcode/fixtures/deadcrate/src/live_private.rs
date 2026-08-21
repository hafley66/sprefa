// rustc: silent. rail: silent (lib.rs calls helper_one).
pub fn helper_one() -> usize { helper_two() }
pub fn helper_two() -> usize { helper_three() }
pub fn helper_three() -> usize { helper_four() }
pub fn helper_four() -> usize { helper_five() }
pub fn helper_five() -> usize { crate::mixed_call_sites::mixed_one() + crate::mixed_call_sites::mixed_two() }

// `mixed_one` and `mixed_two` are named by a shipped site above AND by a test
// site here. The pair pins that the cfg filter is per site: subtracting the
// NAME instead reports mixed_call_sites.rs dead against two shipped calls.
#[cfg(test)]
mod tests {
    use crate::mixed_call_sites::{mixed_one, mixed_two};

    #[test]
    fn exercises_the_shipped_pair() {
        let total = mixed_one() + mixed_two();
        assert_eq!(total, 12);
    }
}
