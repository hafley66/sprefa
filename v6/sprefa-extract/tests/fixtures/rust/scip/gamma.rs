//! scip-ratchet fixture: imports alpha's helper through a use, so scip binds
//! the call where the corpus name-match is ambiguous -> ScipOverride (the ts
//! scip/gamma.ts mirror).
use crate::scip::alpha::helper;

pub fn run() -> u32 {
    helper()
}
