//! Abstraction 3: a GUARD helper called at handler entry. `require_export`
//! encapsulates the export check; every handler that calls it is a gate site.
use crate::models::{PermResult, User};
use crate::perm_service::{Permission, PermissionSet};

/// Guard: returns Err unless the user may export. Handlers call this first.
pub fn require_export(user: &User) -> PermResult {
    let perms = PermissionSet::new(user);
    perms.require(Permission::CanExport)
}

/// Decoy-adjacent guard for the import concept (NOT an export gate).
pub fn require_import(user: &User) -> PermResult {
    let perms = PermissionSet::new(user);
    perms.require(Permission::CanImport)
}
