//! Import handlers. DECOY concept: these gate the IMPORT permission
//! (`can_import`), not export. Negative-control target.
use crate::models::{PermResult, User};
use crate::perm_flags::check_import_flag;
use crate::perm_guard::require_import;

// release 1 style, import concept.
pub fn import_csv(user: &User) -> PermResult {
    check_import_flag(user)?;
    Ok(())
}

// release 3 style, import concept.
pub fn import_bulk(user: &User) -> PermResult {
    require_import(user)?;
    Ok(())
}
