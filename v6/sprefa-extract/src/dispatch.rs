//! The uniform dispatch: one data-driven entry over the `Source` roster. Replaces
//! commit 1-3b's 4 hand-rolled `dispatch_{cst,type,call,df}` (the quadruple Epic U
//! collapses). The generic rayon `dispatch` over many `ExtractJob`s + the arena-
//! per-worker budget land in the parallelism lab (epic 4); this is the single-file
//! piping core both the bin and the snapshot exercise.

use crate::lang::source_for;
use crate::source::{ExtractOutput, FamilyMask};

/// Extract one file's bytes through the first `Source` that matches its path, for
/// exactly the masked families. None when no `Source` matches the path (the bin
/// and the test treat that as "nothing to emit"). The arena(s) are owned inside
/// `Source::extract`; nothing borrowed crosses this call.
pub fn dispatch(path: &str, content: &[u8], mask: FamilyMask) -> Option<ExtractOutput> {
    source_for(path).map(|src| src.extract(path, content, mask))
}
