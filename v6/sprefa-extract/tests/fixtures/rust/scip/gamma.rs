//! scip-ratchet fixture: imports alpha's helper through a use, so scip binds
//! the call where the corpus name-match is ambiguous -> ScipOverride (the ts
//! scip/gamma.ts mirror).
pub fn before_reexport() -> u32 {
    0
}

pub use crate::scip::alpha::helper;

pub fn run() -> u32 {
    // A comment can say use without turning this executable reference into an import.
    helper()
}
