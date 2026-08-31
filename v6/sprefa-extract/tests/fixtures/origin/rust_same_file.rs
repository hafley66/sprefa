// A bare same-file call: no qualifier, no receiver, no import binding, so the
// same-file leg is the only one that can answer.
fn helper() -> u32 {
    7
}

pub fn run() -> u32 {
    helper()
}
