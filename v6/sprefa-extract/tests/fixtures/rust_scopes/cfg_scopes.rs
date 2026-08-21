pub fn shipped() -> usize { shipped_helper() }
fn shipped_helper() -> usize { 1 }

#[cfg(test)]
fn guarded_directly() -> usize { 2 }

#[cfg(test)]
mod tests {
    fn guarded_by_the_module() -> usize { 3 }

    mod deeper {
        fn guarded_by_an_ancestor() -> usize { 4 }
    }
}

#[cfg(any(test, feature = "extra"))]
fn guarded_by_an_any_arm() -> usize { 5 }

#[cfg(feature = "testing")]
fn not_guarded_the_token_is_a_substring() -> usize { 6 }

#[cfg(unix)]
fn not_guarded_a_different_predicate() -> usize { 7 }
