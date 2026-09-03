//! The uniform dispatch: one data-driven entry over the `Source` roster. Replaces
//! commit 1-3b's 4 hand-rolled `dispatch_{cst,type,call,df}` (the quadruple Epic U
//! collapses). The generic rayon `dispatch` over many `ExtractJob`s + the arena-
//! per-worker budget land in the parallelism lab (epic 4); this is the single-file
//! piping core both the bin and the snapshot exercise.

use std::cell::RefCell;
use std::sync::Arc;

use crate::cache::{get_or_extract, CacheKey};
use crate::lang::source_for;
use crate::shape::{content_id_of, ContentId};
use crate::source::{ExtractOutput, FamilyMask};

thread_local! {
    /// The blob id the in-flight `Source::extract` was keyed on, tagged with the
    /// exact slice it was hashed from (read through `extracting_blob`).
    static EXTRACTING: RefCell<Option<(*const u8, usize, ContentId)>> =
        const { RefCell::new(None) };
}

/// Clears `EXTRACTING` on unwind as well as on return, so a panicking door
/// cannot leave one file's blob id visible to the pool thread's next task.
struct ExtractingGuard;

impl Drop for ExtractingGuard {
    fn drop(&mut self) {
        EXTRACTING.with(|slot| *slot.borrow_mut() = None);
    }
}

/// The blob id `dispatch` already hashed these very bytes to. `None` for any
/// other slice, and outside `dispatch` (a direct `Source::extract`, a reparse).
pub(crate) fn extracting_blob(content: &[u8]) -> Option<ContentId> {
    EXTRACTING.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|(start, len, _)| *start == content.as_ptr() && *len == content.len())
            .map(|(_, _, blob)| blob.clone())
    })
}

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
    let blob = content_id_of(content);
    let key = CacheKey::new(blob.clone(), src.name(), mask);
    Some(get_or_extract(key, || {
        EXTRACTING.with(|slot| {
            *slot.borrow_mut() = Some((content.as_ptr(), content.len(), blob));
        });
        let _clear_on_drop = ExtractingGuard;
        Arc::new(src.extract(path, content, mask))
    }))
}
