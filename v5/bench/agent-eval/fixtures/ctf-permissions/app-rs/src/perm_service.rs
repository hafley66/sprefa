//! Abstraction 2: a permission SERVICE keyed by a typed `Permission` enum.
//! Enforcement is `perms.require(Permission::CanExport)?`; the string
//! "can_export" never appears at the call site.
use crate::models::{PermResult, Role, User};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    CanExport,
    CanImport,
    CanView,
}

pub struct PermissionSet<'a> {
    pub user: &'a User,
}

impl<'a> PermissionSet<'a> {
    pub fn new(user: &'a User) -> Self {
        PermissionSet { user }
    }

    pub fn allows(&self, perm: Permission) -> bool {
        match (self.user.role, perm) {
            (Role::Admin, _) => true,
            (Role::Analyst, Permission::CanExport) => true,
            (Role::Analyst, Permission::CanView) => true,
            (Role::Viewer, Permission::CanView) => true,
            _ => false,
        }
    }

    pub fn require(&self, perm: Permission) -> PermResult {
        if self.allows(perm) {
            Ok(())
        } else {
            Err(format!("permission denied: {:?}", perm))
        }
    }
}
