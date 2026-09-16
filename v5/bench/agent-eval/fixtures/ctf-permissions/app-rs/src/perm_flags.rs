//! Abstraction 1 (oldest): a raw boolean-FLAG check straight off the user
//! model. `check_export_flag` inspects `user.can_export` with no service or
//! config indirection; every handler that calls it is a gate site.
use crate::models::{PermResult, User};

pub fn check_export_flag(user: &User) -> PermResult {
    if !user.can_export {
        return Err("export flag denied".into());
    }
    Ok(())
}

/// Decoy-adjacent flag check for the import concept (NOT an export gate).
pub fn check_import_flag(user: &User) -> PermResult {
    if !user.can_import {
        return Err("import flag denied".into());
    }
    Ok(())
}
