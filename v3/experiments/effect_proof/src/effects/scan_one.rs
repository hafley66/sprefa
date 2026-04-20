//! Effect kind: count occurrences of a literal inside a byte buffer.
//!
//! This models the "parse + match" leg of the scan pipeline at the
//! prototype's fidelity (no tree-sitter here — just a byte-count).
//! The point is structural: the same effect kind can be wired to
//! Passthrough, WorkSteal, or BoundedBatched with zero op-code change.

use effect_runtime::EffectKind;

pub struct ScanOne {
    pub bytes: Vec<u8>,
    pub needle: Vec<u8>,
}

impl EffectKind for ScanOne {
    type Response = u64;
}

/// Shared compute function — what each topology wraps. Deliberately
/// pure so all three batchers yield identical output.
pub fn count_needle(req: ScanOne) -> u64 {
    if req.needle.is_empty() {
        return 0;
    }
    let mut n: u64 = 0;
    let hay = &req.bytes;
    let nd = &req.needle;
    if nd.len() > hay.len() {
        return 0;
    }
    for i in 0..=(hay.len() - nd.len()) {
        if &hay[i..i + nd.len()] == nd.as_slice() {
            n += 1;
        }
    }
    n
}
