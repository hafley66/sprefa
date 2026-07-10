//! Export handlers. Each was written in a different release, so each gates the
//! export permission through a different abstraction.
use crate::models::{PermResult, User};
use crate::perm_config::{enforce_rule, RuleContext, EXPORT_RULE};
use crate::perm_flags::check_export_flag;
use crate::perm_guard::require_export;
use crate::perm_service::{Permission, PermissionSet};

// release 1: raw boolean flag (abstraction 1).
pub fn export_csv(user: &User) -> PermResult {
    check_export_flag(user)?;
    Ok(())
}

// release 2: permission service (abstraction 2).
pub fn export_json(user: &User) -> PermResult {
    let perms = PermissionSet::new(user);
    perms.require(Permission::CanExport)?;
    Ok(())
}

// release 3: guard helper (abstraction 3).
pub fn export_bulk(user: &User) -> PermResult {
    require_export(user)?;
    Ok(())
}

// release 4: config-file rule (abstraction 4).
pub fn export_archive(user: &User) -> PermResult {
    let ctx = RuleContext { user };
    enforce_rule(&ctx, EXPORT_RULE)?;
    Ok(())
}
