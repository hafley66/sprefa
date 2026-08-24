//! The uniform dispatch: one data-driven entry over the `Source` roster. Replaces
//! commit 1-3b's 4 hand-rolled `dispatch_{cst,type,call,df}` (the quadruple Epic U
//! collapses). The generic rayon `dispatch` over many `ExtractJob`s + the arena-
//! per-worker budget land in the parallelism lab (epic 4); this is the single-file
//! piping core both the bin and the snapshot exercise.

use std::sync::Arc;

use crate::cache::{get_or_extract, CacheKey};
use crate::lang::source_for;
use crate::shape::content_id_of;
use crate::source::{ExtractOutput, FamilyMask};

/// Extract one file's bytes through the first `Source` that matches its path, for
/// exactly the masked families. None when no `Source` matches the path (the bin
/// and the test treat that as "nothing to emit"). The arena(s) are owned inside
/// `Source::extract`; nothing borrowed crosses this call. The result is the
/// content-keyed cache's shared entry, so a second identical call skips the parse.
pub fn dispatch(path: &str, content: &[u8], mask: FamilyMask) -> Option<Arc<ExtractOutput>> {
    let Some(src) = source_for(path) else {
        tracing::warn!(path, "no Source matches this path; nothing to emit");
        return None;
    };
    let span = tracing::info_span!(
        "extract_file",
        path,
        lang = src.name(),
        bytes = content.len()
    );
    let _entered = span.enter();
    let key = CacheKey::new(content_id_of(content), src.name(), mask);
    Some(get_or_extract(key, || {
        Arc::new(src.extract(path, content, mask))
    }))
}
