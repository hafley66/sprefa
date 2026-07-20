use crate::patterns::{PatternError, PatternPart};
use crate::store::Store;

pub fn check(store: &Store) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    for (id, pattern) in store.patterns.iter().enumerate() {
        if pattern.parts.is_empty() {
            errors.push(format!("pattern {id} has no parts"));
        }
        for pair in pattern.parts.windows(2) {
            if matches!(
                (&pair[0], &pair[1]),
                (PatternPart::Slot(_), PatternPart::Slot(_))
            ) {
                errors.push(format!(
                    "pattern {id} has adjacent slots with ambiguous matching"
                ));
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn error_text(error: PatternError) -> String {
    format!("{error:?}")
}
