// rustc: silent. rail: silent (lib.rs calls exported_one).
pub fn exported_one() -> usize { exported_two() }
pub fn exported_two() -> usize { shared_three() }
pub fn shared_three() -> usize { shared_four() }
pub fn shared_four() -> usize { shared_five() }
pub fn shared_five() -> usize { 1 }

// The ONLY caller of called_only_from_tests.rs, and it is a test. The call is
// spelled as a `let` bind because a call inside `assert_eq!` is macro tokens
// that syn never parses into an expression, so it is no call site at all.
#[cfg(test)]
mod tests {
    #[test]
    fn walks_the_fixture_chain() {
        let total = crate::called_only_from_tests::fixture_step_one();
        assert_eq!(total, 5);
    }
}
