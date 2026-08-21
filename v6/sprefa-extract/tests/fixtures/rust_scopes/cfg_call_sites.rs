pub fn shipped() -> usize {
    let step = shared_step();
    step + only_shipped()
}

fn shared_step() -> usize { 1 }
fn only_shipped() -> usize { 2 }
fn only_tested() -> usize { 3 }
fn deeply_tested() -> usize { 4 }

#[cfg(test)]
mod tests {
    #[test]
    fn exercises_the_pair() {
        let tested = super::only_tested();
        let shared = super::shared_step();
        assert_eq!(tested + shared, 4);
    }

    mod deeper {
        fn reaches_further() -> usize {
            super::super::deeply_tested()
        }
    }
}

#[cfg(feature = "testing")]
fn the_token_is_a_substring() -> usize {
    only_shipped()
}
